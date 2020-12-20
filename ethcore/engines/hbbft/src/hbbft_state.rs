use client_traits::EngineClient;
use common_types::ids::BlockId;
use engine::signer::EngineSigner;
use hbbft::crypto::PublicKey;
use hbbft::honey_badger::{self, HoneyBadgerBuilder};
use hbbft::NetworkInfo;
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
		force: bool,
	) -> Option<()> {
		if !force && self.current_posdao_epoch == get_posdao_epoch(&*client).ok()?.low_u64() {
			// hbbft state is already up to date.
			// @todo Return proper error codes.
			return Some(());
		}

		let posdao_epoch_start = get_posdao_epoch_start(&*client).ok()?;
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
		self.current_posdao_epoch = get_posdao_epoch(&*client).ok()?.low_u64();
		if sks.is_none() {
			info!(target: "engine", "We are not part of the HoneyBadger validator set - running as regular node.");
			return Some(());
		}

		let network_info = synckeygen_to_network_info(&synckeygen, pks, sks)?;
		self.network_info = Some(network_info.clone());
		self.honey_badger = Some(self.new_honey_badger(network_info)?);

		info!(target: "engine", "HoneyBadger Algorithm initialized! Running as validator node.");
		Some(())
	}

	fn skip_to_current_epoch(client: &Arc<dyn EngineClient>, honey_badger: &mut HoneyBadger) {
		if let Some(parent_block_number) = client.block_number(BlockId::Latest) {
			let next_block = parent_block_number + 1;
			honey_badger.skip_to_epoch(next_block);
		} else {
			error!(target: "consensus", "The current chain latest block number could not be obtained.");
		}
	}

	pub fn process_message(
		&mut self,
		client: Arc<dyn EngineClient>,
		sender_id: NodeId,
		message: HbMessage,
	) -> Option<HoneyBadgerStep> {
		// If honey_badger is None we are not a validator, nothing to do.
		let honey_badger = self.honey_badger.as_mut()?;
		HbbftState::skip_to_current_epoch(&client, honey_badger);

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
	) -> Option<HoneyBadgerStep> {
		// If honey_badger is None we are not a validator, nothing to do.
		let honey_badger = self.honey_badger.as_mut()?;
		let network_info = self.network_info.as_ref()?;

		if honey_badger.received_proposals() > network_info.num_faulty() {
			return self.try_send_contribution(client);
		}
		None
	}

	pub fn try_send_contribution(
		&mut self,
		client: Arc<dyn EngineClient>,
	) -> Option<HoneyBadgerStep> {
		// If honey_badger is None we are not a validator, nothing to do.
		let honey_badger = self.honey_badger.as_mut()?;

		// Make sure we are in the most current epoch.
		HbbftState::skip_to_current_epoch(&client, honey_badger);

		// If we already sent a contribution for this epoch, there is nothing to do.
		if honey_badger.has_input() {
			return None;
		}

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
