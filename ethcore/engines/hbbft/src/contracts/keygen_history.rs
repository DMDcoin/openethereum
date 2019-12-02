use crate::NodeId;
use client_traits::EngineClient;
use common_types::ids::BlockId;
use engine::signer::EngineSigner;
use ethereum_types::Address;
use ethkey::Public;
use hbbft::sync_key_gen::{Ack, Error, Part, PubKeyMap, PublicKey, SecretKey, SyncKeyGen};
use hbbft::util::max_faulty;
use hbbft::NetworkInfo;
use parking_lot::RwLock;
use std::str::FromStr;
use std::sync::Arc;
use utils::bound_contract::{BoundContract, CallError};

use_contract!(key_history_contract, "res/key_history_contract.json");

lazy_static! {
	static ref KEYGEN_HISTORY_ADDRESS: Address =
		Address::from_str("8000000000000000000000000000000000000000").unwrap();
}

macro_rules! call_const_key_history {
	($c:ident, $x:ident $(, $a:expr )*) => {
		$c.call_const(key_history_contract::functions::$x::call($($a),*))
	};
}

pub fn engine_signer_to_synckeygen<'a>(
	signer: Arc<RwLock<Option<Box<dyn EngineSigner>>>>,
	pub_keys: PubKeyMap<Public, PublicWrapper>,
) -> Result<(SyncKeyGen<Public, PublicWrapper>, Option<Part>), Error> {
	let wrapper = KeyPairWrapper {
		inner: signer.clone(),
	};
	let public = signer
		.read()
		.as_ref()
		.expect("Signer must be set!")
		.public()
		.expect("Signer's public key must be available!");
	let mut rng = rand::thread_rng();
	let num_nodes = pub_keys.len();
	SyncKeyGen::new(public, wrapper, pub_keys, max_faulty(num_nodes), &mut rng)
}

pub fn synckeygen_to_network_info(
	synckeygen: &SyncKeyGen<Public, PublicWrapper>,
) -> Option<NetworkInfo<NodeId>> {
	let (pks, sks) = synckeygen.generate().ok()?;
	let pub_keys = synckeygen
		.public_keys()
		.keys()
		.map(|p| NodeId(*p))
		.collect::<Vec<_>>();
	Some(NetworkInfo::new(
		NodeId(synckeygen.our_id().clone()),
		sks,
		pks,
		pub_keys,
	))
}

pub fn part_of_address(
	client: &dyn EngineClient,
	address: Address,
	p: Public,
	skg: &mut SyncKeyGen<Public, PublicWrapper>,
) -> Result<(), CallError> {
	let c = BoundContract::bind(client, BlockId::Latest, *KEYGEN_HISTORY_ADDRESS);
	let serialized_part = call_const_key_history!(c, parts, address)?;
	println!("Part for address {}: {:?}", address, serialized_part);
	let deserialized_part: Part = bincode::deserialize(&serialized_part).unwrap();
	let mut rng = rand::thread_rng();
	skg.handle_part(&p, deserialized_part, &mut rng).unwrap();

	Ok(())
}

pub fn acks_of_address(
	client: &dyn EngineClient,
	address: Address,
	p: Public,
	skg: &mut SyncKeyGen<Public, PublicWrapper>,
) -> Result<(), CallError> {
	let c = BoundContract::bind(client, BlockId::Latest, *KEYGEN_HISTORY_ADDRESS);
	let serialized_length = call_const_key_history!(c, get_acks_length, address)?;

	println!(
		"Acks for address {} is of size: {:?}",
		address, serialized_length
	);
	for n in 0..serialized_length.low_u64() {
		let serialized_ack = call_const_key_history!(c, acks, address, n)?;
		println!("Ack #{} for address {}: {:?}", n, address, serialized_ack);
		let deserialized_ack: Ack = bincode::deserialize(&serialized_ack).unwrap();
		skg.handle_ack(&p, deserialized_ack).unwrap();
	}

	Ok(())
}

#[derive(Clone)]
pub struct PublicWrapper {
	pub inner: Public,
}

#[derive(Clone)]
pub struct KeyPairWrapper {
	pub inner: Arc<RwLock<Option<Box<dyn EngineSigner>>>>,
}

impl<'a> PublicKey for PublicWrapper {
	type Error = ethkey::crypto::Error;
	type SecretKey = KeyPairWrapper;
	fn encrypt<M: AsRef<[u8]>, R: rand::Rng>(
		&self,
		msg: M,
		_rng: &mut R,
	) -> Result<Vec<u8>, Self::Error> {
		ethkey::crypto::ecies::encrypt(&self.inner, b"", msg.as_ref())
	}
}

impl<'a> SecretKey for KeyPairWrapper {
	type Error = ethkey::crypto::Error;
	fn decrypt(&self, ct: &[u8]) -> Result<Vec<u8>, Self::Error> {
		self.inner
			.read()
			.as_ref()
			.expect("Signer must be set!")
			.decrypt(b"", ct)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use engine::signer::{from_keypair, EngineSigner};
	use ethkey::{KeyPair, Secret};
	use rustc_hex::FromHex;
	use std::collections::BTreeMap;
	use std::sync::Arc;

	#[test]
	fn test_synckeygen_initialization() {
		// Create a keypair
		let secret = "49c437676c600660905204e5f3710a6db5d3f46e3da9ba5168b9d34b0b787317"
			.from_hex()
			.unwrap();
		let keypair = KeyPair::from_secret(Secret::from_slice(&secret).unwrap())
			.expect("KeyPair generation must succeed");
		let public = keypair.public().clone();
		let wrapper = PublicWrapper {
			inner: public.clone(),
		};

		// Convert it to a EngineSigner trait object
		let signer: Arc<RwLock<Option<Box<dyn EngineSigner>>>> =
			Arc::new(RwLock::new(Some(from_keypair(keypair))));

		// Initialize SyncKeyGen with the EngineSigner wrapper
		let mut pub_keys: BTreeMap<Public, PublicWrapper> = BTreeMap::new();
		pub_keys.insert(public, wrapper);

		assert!(engine_signer_to_synckeygen(signer, Arc::new(pub_keys)).is_ok());
	}
}
