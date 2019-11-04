use client_traits::BlockChainClient;
use common_types::ids::BlockId;
use engine::signer::EngineSigner;
use ethabi::FunctionOutputDecoder;
use ethereum_types::Address;
use ethkey::Public;
use hbbft::sync_key_gen::{Error, Part, PubKeyMap, PublicKey, SecretKey, SyncKeyGen};
use std::str::FromStr;

use_contract!(key_history_contract, "res/key_history_contract.json");

lazy_static! {
	static ref KEYGEN_HISTORY_ADDRESS: Address =
		Address::from_str("8000000000000000000000000000000000000000").unwrap();
}

pub fn part_of_address(client: &dyn BlockChainClient, address: Address) {
	let (data, decoder) = key_history_contract::functions::parts::call(address);
	let return_data = client
		.call_contract(BlockId::Latest, *KEYGEN_HISTORY_ADDRESS, data)
		.unwrap();
	if return_data.is_empty() {
		error!(target: "engine", "A call to KeyGenHistory's 'parts' map returned no data.");
	} else {
		let serialized_part = decoder.decode(&return_data).ok();
		println!("Part for address {}: {:?}", address, serialized_part);
	}
}

pub fn engine_signer_to_synckeygen<'a>(
	signer: &'a Box<dyn EngineSigner>,
	pub_keys: PubKeyMap<Public, KeyPairWrapper<'a>>,
) -> Result<(SyncKeyGen<Public, KeyPairWrapper<'a>>, Option<Part>), Error> {
	let wrapper = KeyPairWrapper { inner: signer };
	let public = signer.public().expect("Signer must be set!");
	let mut rng = rand::thread_rng();
	SyncKeyGen::new(public, wrapper, pub_keys, 0, &mut rng)
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
