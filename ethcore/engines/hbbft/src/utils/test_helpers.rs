use client_traits::{Balance, StateOrBlock};
use common_types::ids::BlockId;
use common_types::transaction::{Action, SignedTransaction, Transaction};
use engine::signer::from_keypair;
use ethcore::client::Client;
use ethcore::miner::{Miner, MinerService};
use ethcore::test_helpers::generate_dummy_client_with_spec;
use ethcore::test_helpers::TestNotify;
use ethereum_types::{Address, U256};
use hbbft::NetworkInfo;
use parity_crypto::publickey::{KeyPair, Public};
use spec::Spec;
use std::sync::Arc;

pub fn hbbft_spec() -> Spec {
	Spec::load(
		&::std::env::temp_dir(),
		include_bytes!("../../res/honey_badger_bft.json") as &[u8],
	)
	.expect(concat!("Chain spec is invalid."))
}

pub fn hbbft_client() -> std::sync::Arc<ethcore::client::Client> {
	generate_dummy_client_with_spec(hbbft_spec)
}

pub struct HbbftTestClient {
	pub client: Arc<Client>,
	pub notify: Arc<TestNotify>,
	pub miner: Arc<Miner>,
	pub keypair: KeyPair,
	pub nonce: U256,
}

impl HbbftTestClient {
	pub fn transfer_to(&mut self, receiver: &Address, amount: &U256) {
		let transaction = create_transfer(&self.keypair, receiver, amount, &self.nonce);
		self.nonce += U256::from(1);
		self.miner
			.import_own_transaction(self.client.as_ref(), transaction.into())
			.unwrap();
	}

	pub fn balance(&self, address: &Address) -> U256 {
		self.client
			.balance(address, StateOrBlock::Block(BlockId::Latest))
			.expect("Querying address balance should always succeed.")
	}

	pub fn address(&self) -> Address {
		self.keypair.address()
	}
}

pub fn create_hbbft_client(keypair: KeyPair) -> HbbftTestClient {
	let client = hbbft_client();
	let miner = client.miner();
	let engine = client.engine();
	let signer = from_keypair(keypair.clone());
	engine.set_signer(Some(signer));
	engine.register_client(Arc::downgrade(&client) as _);
	let notify = Arc::new(TestNotify::default());
	client.add_notify(notify.clone());

	HbbftTestClient {
		client,
		notify,
		miner,
		keypair,
		nonce: U256::from(1048576),
	}
}

pub fn hbbft_client_setup(keypair: KeyPair, net_info: NetworkInfo<Public>) -> HbbftTestClient {
	assert_eq!(keypair.public(), net_info.our_id());
	let client = hbbft_client();

	// Get miner reference
	let miner = client.miner();

	let engine = client.engine();
	// Set the signer *before* registering the client with the engine.
	let signer = from_keypair(keypair.clone());

	engine.set_signer(Some(signer));
	engine.register_client(Arc::downgrade(&client) as _);

	// Register notify object for capturing consensus messages
	let notify = Arc::new(TestNotify::default());
	client.add_notify(notify.clone());

	HbbftTestClient {
		client,
		notify,
		miner,
		keypair,
		nonce: U256::from(1048576),
	}
}

pub fn create_transaction(keypair: &KeyPair) -> SignedTransaction {
	Transaction {
		action: Action::Call(Address::from_low_u64_be(5798439875)),
		value: U256::zero(),
		data: vec![],
		gas: U256::from(100_000),
		gas_price: "10000000000".into(),
		nonce: 1048576.into(),
	}
	.sign(keypair.secret(), None)
}

pub fn create_transfer(
	keypair: &KeyPair,
	receiver: &Address,
	amount: &U256,
	nonce: &U256,
) -> SignedTransaction {
	Transaction {
		action: Action::Call(receiver.clone()),
		value: amount.clone(),
		data: vec![],
		gas: U256::from(100_000),
		gas_price: "10000000000".into(),
		nonce: *nonce,
	}
	.sign(keypair.secret(), None)
}

pub fn inject_transaction(client: &Arc<Client>, miner: &Arc<Miner>, keypair: &KeyPair) {
	let transaction = create_transaction(keypair);
	miner
		.import_own_transaction(client.as_ref(), transaction.into())
		.unwrap();
}
