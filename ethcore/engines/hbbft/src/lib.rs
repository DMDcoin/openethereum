extern crate bincode;
extern crate client_traits;
extern crate common_types;
extern crate engine;
extern crate ethabi;
#[macro_use]
extern crate ethabi_contract;
extern crate ethcore_io as io;
extern crate ethcore_miner;
extern crate ethereum_types;
extern crate hbbft;
extern crate hbbft_testing;
extern crate itertools;
extern crate keccak_hash as hash;
extern crate parity_crypto;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate machine;
extern crate parking_lot;
extern crate rand;
extern crate rustc_hex;
#[macro_use(Serialize, Deserialize)]
extern crate serde;
extern crate ethjson;
extern crate rlp;
extern crate serde_json;

#[cfg(test)]
extern crate ethcore;
#[cfg(test)]
extern crate ethcore_accounts as accounts;
#[cfg(test)]
extern crate proptest;
#[cfg(test)]
extern crate spec;
#[cfg(test)]
extern crate toml;

mod block_reward_hbbft;
mod contracts;
mod contribution;
mod hbbft_engine;
mod hbbft_state;
mod sealing;
mod utils;

pub use hbbft_engine::HoneyBadgerBFT;
use parity_crypto::publickey::Public;
use std::fmt;

#[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct NodeId(pub Public);

impl fmt::Debug for NodeId {
	fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
		write!(f, "{:6}", hex_fmt::HexFmt(&self.0))
	}
}

impl fmt::Display for NodeId {
	fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
		write!(f, "NodeId({})", self.0)
	}
}

#[cfg(test)]
mod tests {
	use crate::contribution::unix_now_secs;
	use crate::utils::test_helpers::{create_hbbft_client, hbbft_client_setup, HbbftTestClient};
	use client_traits::BlockInfo;
	use common_types::ids::BlockId;
	use contracts::staking::tests::{create_staker, is_pool_active};
	use contracts::staking::{get_posdao_epoch, start_time_of_next_phase_transition};
	use contracts::validator_set::{is_pending_validator, mining_by_staking_address};
	use ethereum_types::{Address, H256, U256};
	use hash::keccak;
	use hbbft::NetworkInfo;
	use hbbft_testing::proptest::{gen_seed, TestRng, TestRngSeed};
	use parity_crypto::publickey::{Generator, KeyPair, Public, Random, Secret};
	use proptest::{prelude::ProptestConfig, proptest};
	use rand::{Rng, SeedableRng};
	use std::collections::BTreeMap;
	use std::str::FromStr;

	lazy_static! {
		static ref MASTER_OF_CEREMONIES_KEYPAIR: KeyPair = KeyPair::from_secret(
			Secret::from_str("18f059a4d72d166a96c1edfb9803af258a07b5ec862a961b3a1d801f443a1762")
				.expect("Secret from hex string must succeed")
		)
		.expect("KeyPair generation from secret must succeed");
	}

	fn generate_nodes<R: Rng>(size: usize, rng: &mut R) -> BTreeMap<Public, HbbftTestClient> {
		let keypairs: Vec<KeyPair> = (1..=size)
			.map(|i| {
				let secret = Secret::from(<[u8; 32]>::from(keccak(i.to_string())));
				KeyPair::from_secret(secret).expect("KeyPair generation must succeed")
			})
			.collect();
		let ips_map: BTreeMap<Public, String> = keypairs
			.iter()
			.map(|kp| (*kp.public(), format!("{}", kp.public())))
			.collect();
		let net_infos = NetworkInfo::generate_map(ips_map.keys().cloned(), rng)
			.expect("NetworkInfo generation to always succeed");
		keypairs
			.into_iter()
			.map(|kp| {
				let netinfo = net_infos[kp.public()].clone();
				(*kp.public(), hbbft_client_setup(kp, netinfo))
			})
			.collect()
	}

	// Returns `true` if the node has any unsent messages left.
	fn has_messages(node: &HbbftTestClient) -> bool {
		!node.notify.targeted_messages.read().is_empty()
	}

	#[test]
	fn test_miner_transaction_injection() {
		let mut test_data = create_hbbft_client(MASTER_OF_CEREMONIES_KEYPAIR.clone());

		// Verify that we actually start at block 0.
		assert_eq!(test_data.client.chain().best_block_number(), 0);

		// Inject a transaction, with instant sealing a block will be created right away.
		test_data.create_some_transaction(None);

		// Expect a new block to be created.
		assert_eq!(test_data.client.chain().best_block_number(), 1);

		// Expect one transaction in the block.
		let block = test_data
			.client
			.block(BlockId::Number(1))
			.expect("Block 1 must exist");
		assert_eq!(block.transactions_count(), 1);
	}

	#[test]
	fn test_staking_account_creation() {
		// Create Master of Ceremonies
		let mut moc = create_hbbft_client(MASTER_OF_CEREMONIES_KEYPAIR.clone());

		// Verify the master of ceremony is funded.
		assert!(moc.balance(&moc.address()) > U256::from(10000000));

		// Create a potential validator.
		let miner_1 = create_hbbft_client(Random.generate());

		// Verify the pending validator is unfunded.
		assert_eq!(moc.balance(&miner_1.address()), U256::from(0));

		// Verify that we actually start at block 0.
		assert_eq!(moc.client.chain().best_block_number(), 0);

		let transaction_funds = U256::from(9000000000000000000u64);

		// Inject a transaction, with instant sealing a block will be created right away.
		moc.transfer_to(&miner_1.address(), &transaction_funds);

		// Expect a new block to be created.
		assert_eq!(moc.client.chain().best_block_number(), 1);

		// Verify the pending validator is now funded.
		assert_eq!(moc.balance(&miner_1.address()), transaction_funds);

		// Create staking address
		let staker_1 = create_staker(&mut moc, &miner_1, transaction_funds);

		// Expect two new blocks to be created, one for the transfer of staking funds,
		// one for registering the staker as pool.
		assert_eq!(moc.client.chain().best_block_number(), 3);

		// Expect one transaction in the block.
		let block = moc
			.client
			.block(BlockId::Number(3))
			.expect("Block must exist");
		assert_eq!(block.transactions_count(), 1);

		assert_ne!(
			mining_by_staking_address(moc.client.as_ref(), &staker_1.address())
				.expect("Constant call must succeed."),
			Address::zero()
		);

		// Check if the staking pool is active.
		assert_eq!(
			is_pool_active(moc.client.as_ref(), staker_1.address())
				.expect("Pool active query must succeed."),
			true
		);
	}

	#[test]
	fn test_epoch_transition() {
		// Create Master of Ceremonies
		let mut moc = create_hbbft_client(MASTER_OF_CEREMONIES_KEYPAIR.clone());
		// To avoid performing external transactions with the MoC we create and fund a random address.
		let transactor: KeyPair = Random.generate();

		let genesis_transition_time = start_time_of_next_phase_transition(moc.client.as_ref())
			.expect("Constant call must succeed");

		// Genesis block is at time 0, current unix time must be much larger.
		assert!(genesis_transition_time.as_u64() < unix_now_secs());

		// We should not be in the pending validator set at the genesis block.
		assert!(!is_pending_validator(moc.client.as_ref(), &moc.address())
			.expect("Constant call must succeed"));

		// Fund the transactor.
		// Also triggers the creation of a block.
		// This implicitly calls the block reward contract, which should trigger a phase transition
		// since we already verified that the genesis transition time threshold has been reached.
		let transaction_funds = U256::from(9000000000000000000u64);
		moc.transfer_to(&transactor.address(), &transaction_funds);

		// Expect a new block to be created.
		assert_eq!(moc.client.chain().best_block_number(), 1);

		// Now we should be part of the pending validator set.
		assert!(is_pending_validator(moc.client.as_ref(), &moc.address())
			.expect("Constant call must succeed"));

		// Check if we are still in the first epoch.
		assert_eq!(
			get_posdao_epoch(moc.client.as_ref(), BlockId::Latest).expect("Constant call must succeed"),
			U256::from(0)
		);

		// First the validator realizes it is in the next validator set and sends his part.
		moc.create_some_transaction(Some(&transactor));

		// The part will be included in the block triggered by this transaction, but not part of the global state yet,
		// so it sends the transaction another time.
		moc.create_some_transaction(Some(&transactor));

		// Now the part is part of the global chain state, and we send our acks.
		moc.create_some_transaction(Some(&transactor));

		// The acks will be included in the block triggered by this transaction, but not part of the global state yet.
		moc.create_some_transaction(Some(&transactor));

		// Now the acks are part of the global block state, and the key generation is complete and the next epoch begins
		moc.create_some_transaction(Some(&transactor));

		// At this point we should be in the new epoch.
		assert_eq!(
			get_posdao_epoch(moc.client.as_ref(), BlockId::Latest).expect("Constant call must succeed"),
			U256::from(1)
		);

		// Let's do another one to check if the transition to the new honey badger and keys works.
		moc.create_some_transaction(Some(&transactor));
	}

	fn crank_network_single_step(nodes: &BTreeMap<Public, HbbftTestClient>) {
		for (from, n) in nodes {
			let mut targeted_messages = n.notify.targeted_messages.write();
			for m in targeted_messages.drain(..) {
				nodes
					.get(&m.1.expect("The Message target node id must be set"))
					.expect("Message target not found in nodes map")
					.client
					.engine()
					.handle_message(&m.0, Some(*from))
					.expect("Message handling to succeed");
			}
		}
	}

	fn crank_network(nodes: &BTreeMap<Public, HbbftTestClient>) {
		while nodes.iter().any(|(_, test_data)| has_messages(test_data)) {
			crank_network_single_step(nodes);
		}
	}

	proptest! {
		#![proptest_config(ProptestConfig {
			cases: 1, .. ProptestConfig::default()
		})]

		#[test]
		#[ignore]
		#[allow(clippy::unnecessary_operation)]
		fn test_two_clients(seed in gen_seed()) {
			do_test_two_clients(seed)
		}

		#[test]
		#[ignore]
		#[allow(clippy::unnecessary_operation)]
		fn test_multiple_clients(seed in gen_seed()) {
			do_test_multiple_clients(seed)
		}

		#[test]
		#[ignore]
		#[allow(clippy::unnecessary_operation)]
		fn test_trigger_at_contribution_threshold(seed in gen_seed()) {
			do_test_trigger_at_contribution_threshold(seed)
		}
	}

	fn test_with_size<R: Rng>(rng: &mut R, size: usize) {
		let mut nodes = generate_nodes(size, rng);

		for (_, n) in &mut nodes {
			// Verify that we actually start at block 0.
			assert_eq!(n.client.chain().best_block_number(), 0);
			// Inject transactions to kick off block creation.
			n.create_some_transaction(None);
		}

		// Rudimentary network simulation.
		crank_network(&nodes);

		// All nodes need to have produced a block.
		for (_, n) in &nodes {
			assert_eq!(n.client.chain().best_block_number(), 1);
		}

		// All nodes need to produce the same block with the same hash.
		let mut expected: Option<H256> = None;
		for (_, n) in &nodes {
			match expected {
				None => expected = Some(n.client.chain().best_block_hash()),
				Some(h) => assert_eq!(n.client.chain().best_block_hash(), h),
			}
		}
	}

	fn do_test_two_clients(seed: TestRngSeed) {
		let mut rng = TestRng::from_seed(seed);
		test_with_size(&mut rng, 2);
	}

	fn do_test_multiple_clients(seed: TestRngSeed) {
		let mut rng = TestRng::from_seed(seed);
		let sizes = vec![1, 2, 3, 5, rng.gen_range(6, 10)];
		for size in sizes {
			test_with_size(&mut rng, size);
		}
	}

	fn do_test_trigger_at_contribution_threshold(seed: TestRngSeed) {
		let mut rng = TestRng::from_seed(seed);

		// A network size of 4 allows one adversary.
		// Other nodes should *not* join the epoch if they receive only
		// one contribution, but if 2 or more are received they should!
		let network_size: usize = 4;
		let mut nodes = generate_nodes(network_size, &mut rng);

		// Get the first node as mutable reference and send a transaction to it.
		let first_node: &mut HbbftTestClient = nodes.iter_mut().nth(0).unwrap().1;
		first_node.create_some_transaction(None);

		// Crank the network until no node has any input
		crank_network(&nodes);

		// Cannot re-use the mutable reference to an element of nodes, re-acquire as immutable.
		let first_node = nodes.iter().nth(0).unwrap().1;

		// We expect no new block being generated in this case!
		assert_eq!(first_node.client.chain().best_block_number(), 0);

		// Get the second node and send a transaction to it.
		let second_node = nodes.iter_mut().nth(1).unwrap().1;
		second_node.create_some_transaction(None);

		// Crank the network until no node has any input
		crank_network(&nodes);

		// Need to re-aquire the immutable reference to allow the second node to be acquired mutable.
		let first_node = nodes.iter().nth(0).unwrap().1;

		// This time we do expect a new block has been generated
		assert_eq!(first_node.client.chain().best_block_number(), 1);
	}
}
