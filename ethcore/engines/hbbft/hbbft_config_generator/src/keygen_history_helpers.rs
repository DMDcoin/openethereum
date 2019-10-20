use crate::Enode;
use ethkey::{Public, Secret};
use hbbft::sync_key_gen::{Ack, AckOutcome, Part, PartOutcome, PublicKey, SecretKey, SyncKeyGen};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct KeyPairWrapper {
	pub public: Public,
	pub secret: Secret,
}

impl PublicKey for KeyPairWrapper {
	type Error = ethkey::crypto::Error;
	type SecretKey = KeyPairWrapper;
	fn encrypt<M: AsRef<[u8]>, R: rand::Rng>(
		&self,
		msg: M,
		_rng: &mut R,
	) -> Result<Vec<u8>, Self::Error> {
		ethkey::crypto::ecies::encrypt(&self.public, b"", msg.as_ref())
	}
}

impl SecretKey for KeyPairWrapper {
	type Error = ethkey::crypto::Error;
	fn decrypt(&self, ct: &[u8]) -> Result<Vec<u8>, Self::Error> {
		ethkey::crypto::ecies::decrypt(&self.secret, b"", ct)
	}
}

pub fn generate_keygens<R: rand::Rng>(
	key_pairs: Arc<BTreeMap<Public, KeyPairWrapper>>,
	mut rng: &mut R,
	t: usize,
) -> (
	Vec<SyncKeyGen<Public, KeyPairWrapper>>,
	Vec<(Public, Part)>,
	Vec<(Public, PartOutcome)>,
) {
	// Get SyncKeyGen and Parts
	let (mut sync_keygen, parts): (Vec<_>, Vec<_>) = key_pairs
		.iter()
		.map(|(n, kp)| {
			let s = SyncKeyGen::new(n.clone(), kp.clone(), key_pairs.clone(), t, &mut rng).unwrap();
			(s.0, (n.clone(), s.1.unwrap()))
		})
		.unzip();

	// All SyncKeyGen process all parts, returning Acks
	let acks: Vec<_> = sync_keygen
		.iter_mut()
		.flat_map(|s| {
			parts
				.iter()
				.map(|(n, p)| {
					(
						s.our_id().clone(),
						s.handle_part(n, p.clone(), &mut rng).unwrap(),
					)
				})
				.collect::<Vec<_>>()
		})
		.collect();

	// All SyncKeyGen process all Acks
	let ack_outcomes: Vec<_> = sync_keygen
		.iter_mut()
		.flat_map(|s| {
			acks.iter()
				.map(|(n, p)| match p {
					PartOutcome::Valid(a) => s.handle_ack(n, a.as_ref().unwrap().clone()).unwrap(),
					_ => panic!("Expected Part Outcome to be valid"),
				})
				.collect::<Vec<_>>()
		})
		.collect();

	// Check all Ack Outcomes
	for ao in ack_outcomes {
		if let AckOutcome::Invalid(_) = ao {
			panic!("Expecting Ack Outcome to be valid");
		}
	}

	(sync_keygen, parts, acks)
}

pub fn enodes_to_pub_keys(
	enodes: &BTreeMap<Public, Enode>,
) -> Arc<BTreeMap<Public, KeyPairWrapper>> {
	Arc::new(
		enodes
			.iter()
			.map(|(n, e)| {
				(
					n.clone(),
					KeyPairWrapper {
						public: e.public,
						secret: e.secret.clone(),
					},
				)
			})
			.collect(),
	)
}

#[derive(Serialize, Deserialize)]
struct KeyGenHistoryData {
	parts: BTreeMap<Public, String>,
	acks: BTreeMap<Public, Vec<Ack>>,
}

pub fn key_sync_history_data(
	parts: Vec<(Public, Part)>,
	acks: Vec<(Public, PartOutcome)>,
) -> String {
	let mut data = KeyGenHistoryData {
		parts: BTreeMap::new(),
		acks: BTreeMap::new(),
	};
	for p in parts {
		data.parts.insert(
			p.0,
			serde_json::to_string(&p.1).expect("Part has to serialize"),
		);
	}
	for a in acks {
		match a.1 {
			PartOutcome::Valid(ack_option) => {
				if let Some(ack) = ack_option {
					let v = data.acks.entry(a.0).or_insert(Vec::new());
					v.push(ack);
				} else {
					panic!("Unexpected valid part outcome without Ack message");
				}
			}
			_ => panic!("Expected Part Outcome to be valid"),
		}
	}
	serde_json::to_string(&data).expect("Keygen History must convert to JSON")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_keygen_history_data_serde() {
		let mut rng = rand::thread_rng();
		let (secret, public, _) = crate::create_account();
		let keypair = KeyPairWrapper { public, secret };
		let mut pub_keys: BTreeMap<Public, KeyPairWrapper> = BTreeMap::new();
		pub_keys.insert(public, keypair.clone());
		let (_, parts, _) = generate_keygens(Arc::new(pub_keys), &mut rng, 1);

		let part = parts
			.iter()
			.nth(0)
			.expect("At least one part needs to exist");
		let part_serialized = serde_json::to_string(&part.1).expect("Part has to serialize");
		let part_deserialized: Part =
			serde_json::from_str(&part_serialized).expect("Deserialization expected to succeed");
		assert_eq!(part.1, part_deserialized);
	}
}
