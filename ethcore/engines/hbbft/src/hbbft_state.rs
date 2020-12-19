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
use crate::contribution::Contribution;
use crate::NodeId;

type HoneyBadger = honey_badger::HoneyBadger<Contribution, NodeId>;

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
			return None;
		}

		let posdao_epoch_start = get_posdao_epoch_start(&*client).ok()?;
		let synckeygen = initialize_synckeygen(
			&*client,
			signer,
			BlockId::Number(posdao_epoch_start.low_u64()),
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
}
