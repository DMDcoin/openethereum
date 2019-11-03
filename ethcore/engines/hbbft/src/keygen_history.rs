use engine::signer::EngineSigner;
use hbbft::sync_key_gen::{Error, Part, PubKeyMap, PublicKey, SecretKey, SyncKeyGen};

pub fn engine_signer_to_synckeygen<'a>(
	signer: &'a Box<dyn EngineSigner>,
	pub_keys: PubKeyMap<ethkey::Public, KeyPairWrapper<'a>>,
) -> Result<(SyncKeyGen<ethkey::Public, KeyPairWrapper<'a>>, Option<Part>), Error> {
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
		//ethkey::crypto::ecies::decrypt(&self.secret, b"", ct)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use engine::signer::{from_keypair, EngineSigner};
	use ethkey::{KeyPair, Secret};
	use std::collections::BTreeMap;
	use std::sync::Arc;

	#[test]
	fn test_synckeygen_initialization() {
		// Create a keypair
		let keypair = KeyPair::from_secret(Secret::from_slice(&[3u8; 32]).unwrap())
			.expect("KeyPair generation must succeed");

		// Convert it to a EngineSigner trait object
		let signer: Box<dyn EngineSigner> = from_keypair(keypair);
		let wrapper = KeyPairWrapper { inner: &signer };

		// Initialize SyncKeyGen with the EngineSigner wrapper
		let public = signer.public().unwrap();
		let mut pub_keys: BTreeMap<ethkey::Public, KeyPairWrapper> = BTreeMap::new();
		pub_keys.insert(public, wrapper.clone());

		assert!(engine_signer_to_synckeygen(&signer, Arc::new(pub_keys)).is_ok());
	}
}
