use crate::Enode;
use parity_crypto::publickey::{Address, Public, Secret, public_to_address};
use hbbft::sync_key_gen::{AckOutcome, Part, PartOutcome, PublicKey, SecretKey, SyncKeyGen};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct KeyPairWrapper {
	pub public: Public,
	pub secret: Secret,
}

impl PublicKey for KeyPairWrapper {
	type Error = parity_crypto::publickey::Error;
	type SecretKey = KeyPairWrapper;
	fn encrypt<M: AsRef<[u8]>, R: rand::Rng>(
		&self,
		msg: M,
		_rng: &mut R,
	) -> Result<Vec<u8>, Self::Error> {
		parity_crypto::publickey::ecies::encrypt(&self.public, b"", msg.as_ref())
	}
}

impl SecretKey for KeyPairWrapper {
	type Error = parity_crypto::publickey::Error;
	fn decrypt(&self, ct: &[u8]) -> Result<Vec<u8>, Self::Error> {
		parity_crypto::publickey::ecies::decrypt(&self.secret, b"", ct)
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
	parts: BTreeMap<Address, Vec<u8>>,
	acks: BTreeMap<Address, Vec<Vec<u8>>>,
}

pub fn key_sync_history_data(
	parts: Vec<(Public, Part)>,
	acks: Vec<(Public, PartOutcome)>,
) -> String {
	let mut data = KeyGenHistoryData {
		parts: BTreeMap::new(),
		acks: BTreeMap::new(),
	};

	let mut parts_total_bytes = 0;
	let mut num_parts = 0;
	let mut acks_total_bytes = 0;
	let mut num_acks = 0;

	for p in parts {
		let serialized = bincode::serialize(&p.1).expect("Part has to serialize");
		parts_total_bytes += serialized.len();
		num_parts += 1;
		data.parts.insert(public_to_address(&p.0), serialized);
	}
	for a in acks {
		match a.1 {
			PartOutcome::Valid(ack_option) => {
				if let Some(ack) = ack_option {
					let v = data.acks.entry(public_to_address(&a.0)).or_insert(Vec::new());
					let ack_serialized = bincode::serialize(&ack).expect("Ack has to serialize");
					acks_total_bytes += ack_serialized.len();
					num_acks += 1;
					v.push(ack_serialized);
				} else {
					panic!("Unexpected valid part outcome without Ack message");
				}
			}
			_ => panic!("Expected Part Outcome to be valid"),
		}
	}

	println!(
		"{} parts, total number of bytes: {}",
		num_parts, parts_total_bytes
	);
	println!(
		"{} Acks, total number of bytes: {}",
		num_acks, acks_total_bytes
	);
	println!(
		"Total number of bytes: {}",
		parts_total_bytes + acks_total_bytes
	);
	println!(
		"{},{},{},{},{}",
		num_parts,
		num_acks,
		parts_total_bytes,
		acks_total_bytes,
		parts_total_bytes + acks_total_bytes
	);

	serde_json::to_string(&data).expect("Keygen History must convert to JSON")
}

#[cfg(test)]
mod tests {
	use super::*;
	use bincode;

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
		let part_ser = bincode::serialize(&part.1).expect("Part has to serialize");
		let part_deser: Part =
			bincode::deserialize(&part_ser).expect("Deserialization expected to succeed");
		assert_eq!(part.1, part_deser);
	}
}
