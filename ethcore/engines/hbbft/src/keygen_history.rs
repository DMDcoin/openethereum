use client_traits::BlockChainClient;
use common_types::ids::BlockId;
use engine::signer::EngineSigner;
use ethabi::FunctionOutputDecoder;
use ethereum_types::Address;
use ethkey::Public;
use hbbft::sync_key_gen::{Ack, Error, Part, PubKeyMap, PublicKey, SecretKey, SyncKeyGen};
use std::str::FromStr;

use_contract!(key_history_contract, "res/key_history_contract.json");

lazy_static! {
	static ref KEYGEN_HISTORY_ADDRESS: Address =
		Address::from_str("8000000000000000000000000000000000000000").unwrap();
}

pub fn engine_signer_to_synckeygen<'a>(
	signer: &'a Box<dyn EngineSigner>,
	pub_keys: PubKeyMap<Public, KeyPairWrapper<'a>>,
) -> Result<(SyncKeyGen<Public, KeyPairWrapper<'a>>, Option<Part>), Error> {
	let wrapper = KeyPairWrapper { inner: signer };
	let public = signer.public().expect("Signer must be set!");
	let mut rng = rand::thread_rng();
	let num_nodes = pub_keys.len();
	SyncKeyGen::new(public, wrapper, pub_keys, (num_nodes - 1) / 3, &mut rng)
}

pub fn part_of_address(
	client: &dyn BlockChainClient,
	address: Address,
	p: Public,
	skg: &mut SyncKeyGen<Public, KeyPairWrapper<'_>>,
) {
	let (data, decoder) = key_history_contract::functions::parts::call(address);
	let return_data = client
		.call_contract(BlockId::Latest, *KEYGEN_HISTORY_ADDRESS, data)
		.unwrap();
	if return_data.is_empty() {
		error!(target: "engine", "A call to KeyGenHistory's 'parts' map returned no data.");
	} else {
		let serialized_part = decoder.decode(&return_data).unwrap();
		println!("Part for address {}: {:?}", address, serialized_part);
		let deserialized_part: Part = bincode::deserialize(&serialized_part).unwrap();
		let mut rng = rand::thread_rng();
		skg.handle_part(&p, deserialized_part, &mut rng).unwrap();
	}
}

pub fn acks_of_address(
	client: &dyn BlockChainClient,
	address: Address,
	p: Public,
	skg: &mut SyncKeyGen<Public, KeyPairWrapper<'_>>,
) {
	let (data, decoder) = key_history_contract::functions::get_acks_length::call(address);
	let return_data = client
		.call_contract(BlockId::Latest, *KEYGEN_HISTORY_ADDRESS, data)
		.unwrap();
	if return_data.is_empty() {
		error!(target: "engine", "A call to KeyGenHistory's 'acks' map returned no data.");
	} else {
		let serialized_length = decoder.decode(&return_data).unwrap();
		println!(
			"Acks for address {} is of size: {:?}",
			address, serialized_length
		);
		for n in 0..serialized_length.low_u64() {
			let (data, decoder) = key_history_contract::functions::acks::call(address, n);
			let return_data = client
				.call_contract(BlockId::Latest, *KEYGEN_HISTORY_ADDRESS, data)
				.unwrap();
			if return_data.is_empty() {
				error!(target: "engine", "A call to KeyGenHistory's 'acks' map returned no data.");
			} else {
				let serialized_ack = decoder.decode(&return_data).unwrap();
				println!("Ack #{} for address {}: {:?}", n, address, serialized_ack);
				let deserialized_ack: Ack = bincode::deserialize(&serialized_ack).unwrap();
				skg.handle_ack(&p, deserialized_ack).unwrap();
			}
		}
	}
}

#[derive(Clone)]
pub struct KeyPairWrapper<'a> {
	pub inner: &'a Box<dyn EngineSigner>,
}

impl<'a> PublicKey for KeyPairWrapper<'a> {
	type Error = ethkey::crypto::Error;
	type SecretKey = KeyPairWrapper<'a>;
	fn encrypt<M: AsRef<[u8]>, R: rand::Rng>(
		&self,
		msg: M,
		_rng: &mut R,
	) -> Result<Vec<u8>, Self::Error> {
		let public = self.inner.public().expect("Engine Signer must be set");
		ethkey::crypto::ecies::encrypt(&public, b"", msg.as_ref())
	}
}

impl<'a> SecretKey for KeyPairWrapper<'a> {
	type Error = ethkey::crypto::Error;
	fn decrypt(&self, ct: &[u8]) -> Result<Vec<u8>, Self::Error> {
		self.inner.decrypt(b"", ct)
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

		// Convert it to a EngineSigner trait object
		let signer: Box<dyn EngineSigner> = from_keypair(keypair);
		let wrapper = KeyPairWrapper { inner: &signer };

		// Initialize SyncKeyGen with the EngineSigner wrapper
		let public = signer.public().unwrap();
		let mut pub_keys: BTreeMap<Public, KeyPairWrapper> = BTreeMap::new();
		pub_keys.insert(public, wrapper.clone());

		assert!(engine_signer_to_synckeygen(&signer, Arc::new(pub_keys)).is_ok());
	}
}
