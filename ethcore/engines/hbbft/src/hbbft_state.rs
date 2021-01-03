use client_traits::EngineClient;
use common_types::ids::BlockId;
use engine::signer::EngineSigner;
use hbbft::crypto::PublicKey;
use hbbft::honey_badger::{self, HoneyBadgerBuilder};
use hbbft::{Epoched, NetworkInfo};
use parking_lot::RwLock;
use std::sync::Arc;

use crate::contracts::keygen_history::{initialize_synckeygen, synckeygen_to_network_info};
use crate::contracts::staking::{get_posdao_epoch, get_posdao_epoch_start};
use crate::contracts::validator_set::ValidatorType;
use crate::contribution::Contribution;
use crate::NodeId;

pub type HbMessage = honey_badger::Message<NodeId>;
pub(crate) type HoneyBadger = honey_badger::HoneyBadger<Contribution, NodeId>;
pub(crate) type Batch = honey_badger::Batch<Contribution, NodeId>;
pub(crate) type HoneyBadgerStep = honey_badger::Step<Contribution, NodeId>;

pub(crate) struct HbbftState {
	pub network_info: Option<NetworkInfo<NodeId>>,
	pub honey_badger: Option<HoneyBadger>,
	pub public_master_key: Option<PublicKey>,
	pub current_posdao_epoch: u64,
}

impl HbbftState {
	pub fn new() -> Self {
		HbbftState {
			network_info: None,
			honey_badger: None,
			public_master_key: None,
			current_posdao_epoch: 0,
		}
	}

	fn new_honey_badger(&self, network_info: NetworkInfo<NodeId>) -> Option<HoneyBadger> {
		let mut builder: HoneyBadgerBuilder<Contribution, _> =
			HoneyBadger::builder(Arc::new(network_info));
		return Some(builder.build());
	}

	pub fn update_honeybadger(
		&mut self,
		client: Arc<dyn EngineClient>,
		signer: &Arc<RwLock<Option<Box<dyn EngineSigner>>>>,
		block_id: BlockId,
		force: bool,
	) -> Option<()> {
		let target_posdao_epoch = get_posdao_epoch(&*client, block_id).ok()?.low_u64();
		if !force && self.current_posdao_epoch == target_posdao_epoch {
			// hbbft state is already up to date.
			// @todo Return proper error codes.
			return Some(());
		}

		let posdao_epoch_start = get_posdao_epoch_start(&*client, block_id).ok()?;
		let synckeygen = initialize_synckeygen(
			&*client,
			signer,
			BlockId::Number(posdao_epoch_start.low_u64()),
			ValidatorType::Current,
		)
		.ok()?;
		assert!(synckeygen.is_ready());

		let (pks, sks) = synckeygen.generate().ok()?;
		self.public_master_key = Some(pks.public_key());
		// Clear network info and honey badger instance, since we may not be in this POSDAO epoch any more.
		self.network_info = None;
		self.honey_badger = None;
		// Set the current POSDAO epoch #
		self.current_posdao_epoch = target_posdao_epoch;
		trace!(target: "engine", "Switched hbbft state to epoch {}.", self.current_posdao_epoch);
		if sks.is_none() {
			trace!(target: "engine", "We are not part of the HoneyBadger validator set - running as regular node.");
			return Some(());
		}

		let network_info = synckeygen_to_network_info(&synckeygen, pks, sks)?;
		self.network_info = Some(network_info.clone());
		self.honey_badger = Some(self.new_honey_badger(network_info)?);

		trace!(target: "engine", "HoneyBadger Algorithm initialized! Running as validator node.");
		Some(())
	}

	fn skip_to_current_epoch(honey_badger: &mut HoneyBadger, block_nr: u64) {
		let next_block = block_nr + 1;
		if next_block != honey_badger.epoch() {
			trace!(target: "consensus", "Skipping honey_badger forward to epoch(block) {}, was at epoch(block) {}.", next_block, honey_badger.epoch());
		}
		honey_badger.skip_to_epoch(next_block);
	}

	pub fn process_message(
		&mut self,
		client: Arc<dyn EngineClient>,
		signer: &Arc<RwLock<Option<Box<dyn EngineSigner>>>>,
		sender_id: NodeId,
		message: HbMessage,
	) -> Option<HoneyBadgerStep> {
		// Ensure we evaluate at the same block # in the entire upward call graph to avoid inconsistent state.
		let latest_block_number = client.block_number(BlockId::Latest)?;

		// Update honey_badger *before* trying to use it to make sure we use the data
		// structures matching the current epoch.
		self.update_honeybadger(
			client.clone(),
			signer,
			BlockId::Number(latest_block_number),
			false,
		);
		// If honey_badger is None we are not a validator, nothing to do.
		let honey_badger = self.honey_badger.as_mut()?;
		HbbftState::skip_to_current_epoch(honey_badger, latest_block_number);

		// Note that if the message is for a future epoch we do not know if the current honey_badger
		// instance is the correct one to use. Tt may change if the the POSDAO epoch changes, causing
		// consensus messages to get lost.
		if message.epoch() > honey_badger.epoch() {
			error!(target: "consensus", "The current chain latest block number could not be obtained.");
			panic!("Caching of future epochs not implemented yet!");
		}

		if let Ok(step) = honey_badger.handle_message(&sender_id, message) {
			Some(step)
		} else {
			// TODO: Report consensus step errors
			error!(target: "consensus", "Error on handling HoneyBadger message.");
			None
		}
	}

	pub fn contribute_if_contribution_threshold_reached(
		&mut self,
		client: Arc<dyn EngineClient>,
		signer: &Arc<RwLock<Option<Box<dyn EngineSigner>>>>,
	) -> Option<HoneyBadgerStep> {
		// If honey_badger is None we are not a validator, nothing to do.
		let honey_badger = self.honey_badger.as_mut()?;
		let network_info = self.network_info.as_ref()?;

		if honey_badger.received_proposals() > network_info.num_faulty() {
			return self.try_send_contribution(client, signer);
		}
		None
	}

	pub fn try_send_contribution(
		&mut self,
		client: Arc<dyn EngineClient>,
		signer: &Arc<RwLock<Option<Box<dyn EngineSigner>>>>,
	) -> Option<HoneyBadgerStep> {
		// Ensure we evaluate at the same block # in the entire upward call graph to avoid inconsistent state.
		let latest_block_number = client.block_number(BlockId::Latest)?;

		// Update honey_badger *before* trying to use it to make sure we use the data
		// structures matching the current epoch.
		self.update_honeybadger(
			client.clone(),
			signer,
			BlockId::Number(latest_block_number),
			false,
		);

		// If honey_badger is None we are not a validator, nothing to do.
		let honey_badger = self.honey_badger.as_mut()?;

		// Make sure we are in the most current epoch.
		HbbftState::skip_to_current_epoch(honey_badger, latest_block_number);

		// If we already sent a contribution for this epoch, there is nothing to do.
		if honey_badger.has_input() {
			return None;
		}

		// If the parent block of the block we would contribute to is not in the hbbft state's
		// epoch we cannot start to contribute, since we would write into a hbbft instance
		// which will be destroyed.
		let posdao_epoch = get_posdao_epoch(&*client, BlockId::Number(honey_badger.epoch() - 1))
			.ok()?
			.low_u64();
		if self.current_posdao_epoch != posdao_epoch {
			trace!(target: "consensus", "hbbft_state epoch mismatch: hbbft_state epoch is {}, honey badger instance epoch is: {}.", 
				   self.current_posdao_epoch, posdao_epoch);
			return None;
		}

		trace!(target: "consensus", "Writing contribution for hbbft epoch(block) {}.", honey_badger.epoch());

		// Now we can select the transactions to include in our contribution.
		// TODO: Select a random *subset* of transactions to propose
		let input_contribution = Contribution::new(
			&client
				.queued_transactions()
				.iter()
				.map(|txn| txn.signed().clone())
				.collect(),
		);

		let mut rng = rand::thread_rng();
		let step = honey_badger.propose(&input_contribution, &mut rng);
		match step {
			Ok(step) => Some(step),
			_ => {
				// TODO: Report detailed consensus step errors
				error!(target: "consensus", "Error on proposing Contribution.");
				None
			}
		}
	}
}
