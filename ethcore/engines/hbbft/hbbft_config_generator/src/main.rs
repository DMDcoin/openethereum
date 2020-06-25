extern crate bincode;
#[macro_use]
extern crate clap;
extern crate client_traits;
extern crate ethcore;
extern crate ethereum_types;
extern crate ethkey;
extern crate ethstore;
extern crate hbbft;
extern crate parity_crypto;
extern crate rand;
extern crate rustc_hex;
extern crate serde;
extern crate serde_json;
extern crate toml;

mod keygen_history_helpers;

use clap::{App, Arg};
use ethstore::{KeyFile, SafeAccount};
use hbbft::crypto::serde_impl::SerdeSecret;
use hbbft::sync_key_gen::SyncKeyGen;
use keygen_history_helpers::{
	enodes_to_pub_keys, generate_keygens, key_sync_history_data, KeyPairWrapper,
};
use parity_crypto::publickey::{Address, Generator, KeyPair, Public, Random, Secret};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use toml::{map::Map, Value};

pub fn create_account() -> (Secret, Public, Address) {
	let acc = Random
		.generate();
	(
		acc.secret().clone(),
		acc.public().clone(),
		acc.address().clone(),
	)
}

pub struct Enode {
	secret: Secret,
	public: Public,
	address: Address,
	idx: usize,
	ip: String,
}

impl ToString for Enode {
	fn to_string(&self) -> String {
		// Example:
		// enode://30ccdeb8c31972f570e4eea0673cd08cbe7cefc5de1d70119b39c63b1cba33b48e494e9916c0d1eab7d296774f3573da46025d1accdef2f3690bc9e6659a34b4@192.168.0.101:30300
		let port = 30300usize + self.idx;
		format!("enode://{:x}@{}:{}", self.public, self.ip, port)
	}
}

fn generate_enodes(num_nodes: usize, external_ip: Option<&str>) -> BTreeMap<Public, Enode> {
	let mut map = BTreeMap::new();
	for i in 0..num_nodes {
		// Note: node 0 is a regular full node (not a validator) in the testnet setup, so we start at index 1.
		let idx = i + 1;
		let ip = match external_ip {
			Some(ip) => ip,
			None => "127.0.0.1",
		};
		let (secret, public, address) = create_account();
		map.insert(
			public,
			Enode {
				secret,
				public,
				address,
				idx,
				ip: ip.into(),
			},
		);
	}
	map
}

fn to_toml_array(vec: Vec<&str>) -> Value {
	Value::Array(vec.iter().map(|s| Value::String(s.to_string())).collect())
}

fn to_toml<N>(
	keygen: &SyncKeyGen<N, KeyPairWrapper>,
	enodes_map: &BTreeMap<N, Enode>,
	i: usize,
	config_type: &ConfigType,
	external_ip: Option<&str>,
	signer_address: &Address,
) -> Value
where
	N: hbbft::NodeIdT + Serialize,
{
	let base_port = 30300i64;
	let base_rpc_port = 8540i64;
	let base_ws_port = 9540i64;
	let generated_keys = keygen.generate().unwrap();

	let mut parity = Map::new();
	match config_type {
		ConfigType::PosdaoSetup => {
			parity.insert("chain".into(), Value::String("./spec/spec.json".into()));
			parity.insert("chain".into(), Value::String("./spec/spec.json".into()));
			let node_data_path = format!("parity-data/node{}", i);
			parity.insert("base_path".into(), Value::String(node_data_path));
		}
		_ => {
			parity.insert("chain".into(), Value::String("spec.json".into()));
			parity.insert("chain".into(), Value::String("spec.json".into()));
			let node_data_path = "data".to_string();
			parity.insert("base_path".into(), Value::String(node_data_path));
		}
	}

	let mut ui = Map::new();
	ui.insert("disable".into(), Value::Boolean(true));

	let mut network = Map::new();
	network.insert("port".into(), Value::Integer(base_port + i as i64));
	match config_type {
		ConfigType::PosdaoSetup => {
			network.insert(
				"reserved_peers".into(),
				Value::String("parity-data/reserved-peers".into()),
			);
		}
		_ => {
			network.insert(
				"reserved_peers".into(),
				Value::String("reserved-peers".into()),
			);
		}
	}

	match external_ip {
		Some(extip) => {
			network.insert("allow_ips".into(), Value::String("public".into()));
			network.insert("nat".into(), Value::String(format!("extip:{}", extip)));
		}
		None => {
			network.insert("nat".into(), Value::String("none".into()));
			network.insert("interface".into(), Value::String("all".into()));
		}
	}

	let mut rpc = Map::new();
	rpc.insert("interface".into(), Value::String("all".into()));
	rpc.insert("cors".into(), to_toml_array(vec!["all"]));
	rpc.insert("hosts".into(), to_toml_array(vec!["all"]));
	let apis = to_toml_array(vec![
		"web3",
		"eth",
		"pubsub",
		"net",
		"parity",
		"parity_set",
		"parity_pubsub",
		"personal",
		"traces",
		"rpc",
		"shh",
		"shh_pubsub",
	]);
	rpc.insert("apis".into(), apis);
	rpc.insert("port".into(), Value::Integer(base_rpc_port + i as i64));

	let mut websockets = Map::new();
	websockets.insert("interface".into(), Value::String("all".into()));
	websockets.insert("origins".into(), to_toml_array(vec!["all"]));
	websockets.insert("port".into(), Value::Integer(base_ws_port + i as i64));

	let mut ipc = Map::new();
	ipc.insert("disable".into(), Value::Boolean(true));

	let mut secretstore = Map::new();
	secretstore.insert("disable".into(), Value::Boolean(true));

	let signer_address = format!("{:?}", signer_address);

	let mut account = Map::new();
	match config_type {
		ConfigType::PosdaoSetup => {
			account.insert(
				"unlock".into(),
				to_toml_array(vec![
					"0xbbcaa8d48289bb1ffcf9808d9aa4b1d215054c78",
					"0x32e4e4c7c5d1cea5db5f9202a9e4d99e56c91a24",
				]),
			);
			account.insert("password".into(), to_toml_array(vec!["config/password"]));
		}
		ConfigType::Docker => {
			account.insert("unlock".into(), to_toml_array(vec![&signer_address]));
			account.insert("password".into(), to_toml_array(vec!["password.txt"]));
		}
		_ => (),
	}

	let mut mining = Map::new();

	if config_type != &ConfigType::Rpc {
		mining.insert("engine_signer".into(), Value::String(signer_address));
	}

	mining.insert("force_sealing".into(), Value::Boolean(true));
	mining.insert("min_gas_price".into(), Value::Integer(1000000000));
	mining.insert("reseal_on_txs".into(), Value::String("none".into()));
	mining.insert("extra_data".into(), Value::String("Parity".into()));
	mining.insert("reseal_min_period".into(), Value::Integer(0));

	let mut misc = Map::new();
	misc.insert("logging".into(), Value::String("".into()));
	misc.insert("log_file".into(), Value::String("parity.log".into()));

	let mut map = Map::new();
	map.insert("parity".into(), Value::Table(parity));
	map.insert("ui".into(), Value::Table(ui));
	map.insert("network".into(), Value::Table(network));
	map.insert("rpc".into(), Value::Table(rpc));
	map.insert("websockets".into(), Value::Table(websockets));
	map.insert("ipc".into(), Value::Table(ipc));
	map.insert("secretstore".into(), Value::Table(secretstore));
	map.insert("account".into(), Value::Table(account));
	map.insert("mining".into(), Value::Table(mining));
	map.insert("misc".into(), Value::Table(misc));
	Value::Table(map)
}

arg_enum! {
	#[derive(Debug, PartialEq)]
	enum ConfigType {
		PosdaoSetup,
		Docker,
		Rpc
	}
}

fn write_json_for_secret(secret: Secret, filename: String) {
	let json_key: KeyFile = SafeAccount::create(
		&KeyPair::from_secret(secret).unwrap(),
		[0u8; 16],
		&"test".into(),
		10240,
		"Test".to_owned(),
		"{}".to_owned(),
	)
	.expect("json key object creation should succeed")
	.into();

	let serialized_json_key =
		serde_json::to_string(&json_key).expect("json key object serialization should succeed");
	fs::write(filename, serialized_json_key).expect("Unable to write json key file");
}

fn main() {
	let matches = App::new("hbbft parity config generator")
		.version("1.0")
		.author("David Forstenlechner <dforsten@gmail.com>")
		.about("Generates n toml files for running a hbbft validator node network")
		.arg(
			Arg::with_name("INPUT")
				.help("The number of config files to generate")
				.required(true)
				.index(1),
		)
		.arg(
			Arg::from_usage("<configtype> 'The ConfigType to use'")
				.possible_values(&ConfigType::variants())
				.index(2),
		)
		.arg(
			Arg::from_usage("<extip> 'Optional external ip to configure'")
				.required(false)
				.index(3),
		)
		.get_matches();

	let num_nodes: usize = matches
		.value_of("INPUT")
		.expect("Number of nodes input required")
		.parse()
		.expect("Input must be of integer type");

	println!("Number of config files to generate: {}", num_nodes);

	let config_type =
		value_t!(matches.value_of("configtype"), ConfigType).unwrap_or(ConfigType::PosdaoSetup);

	let external_ip = matches.value_of("extip");

	let enodes_map = generate_enodes(num_nodes, external_ip);
	let mut rng = rand::thread_rng();

	let pub_keys = enodes_to_pub_keys(&enodes_map);
	let (sync_keygen, parts, acks) = generate_keygens(pub_keys, &mut rng, (num_nodes - 1) / 3);

	let mut reserved_peers = String::new();
	for keygen in sync_keygen.iter() {
		let enode = enodes_map
			.get(keygen.our_id())
			.expect("validator id must be mapped");
		writeln!(&mut reserved_peers, "{}", enode.to_string())
			.expect("enode should be written to the reserved peers string");
		let i = enode.idx;
		let file_name = format!("hbbft_validator_{}.toml", i);
		let toml_string = toml::to_string(&to_toml(
			keygen,
			&enodes_map,
			i,
			&config_type,
			external_ip,
			&enode.address,
		))
		.expect("TOML string generation should succeed");
		fs::write(file_name, toml_string).expect("Unable to write config file");

		let file_name = format!("hbbft_validator_key_{}", i);
		fs::write(file_name, enode.secret.to_hex()).expect("Unable to write key file");

		write_json_for_secret(
			enode.secret.clone(),
			format!("hbbft_validator_key_{}.json", i),
		);
	}
	// Write rpc node config
	let rpc_string = toml::to_string(&to_toml(
		sync_keygen
			.iter()
			.nth(0)
			.expect("At least one SyncKeyGen entry must exist"),
		&enodes_map,
		0,
		&ConfigType::Rpc,
		external_ip,
		&Address::default(),
	))
	.expect("TOML string generation should succeed");
	fs::write("rpc_node.toml", rpc_string).expect("Unable to write rpc config file");

	// Write reserved peers file
	fs::write("reserved-peers", reserved_peers).expect("Unable to write reserved_peers file");

	// Write the password file
	fs::write("password.txt", "test").expect("Unable to write password.txt file");

	fs::write(
		"keygen_history.json",
		key_sync_history_data(parts, acks, enodes_map),
	)
	.expect("Unable to write keygen history data file");
}

#[cfg(test)]
mod tests {
	use super::*;
	use hbbft::crypto::{PublicKeySet, SecretKeyShare};
	use hbbft::sync_key_gen::{AckOutcome, PartOutcome};
	use rand;
	use serde::Deserialize;
	use std::collections::BTreeMap;
	use std::sync::Arc;

	#[derive(Deserialize)]
	struct TomlHbbftOptions {
		pub mining: client_traits::HbbftOptions,
	}

	fn compare<'a, N>(keygen: &SyncKeyGen<N, KeyPairWrapper>, options: &'a TomlHbbftOptions)
	where
		N: hbbft::NodeIdT + Serialize + Deserialize<'a>,
	{
		let generated_keys = keygen.generate().unwrap();

		// Parse and compare the Secret Key Share
		let secret_key_share: SerdeSecret<SecretKeyShare> =
			serde_json::from_str(&options.mining.hbbft_secret_share).unwrap();
		assert_eq!(generated_keys.1.unwrap(), *secret_key_share);

		// Parse and compare the Public Key Set
		let pks: PublicKeySet = serde_json::from_str(&options.mining.hbbft_public_key_set).unwrap();
		assert_eq!(generated_keys.0, pks);

		// Parse and compare the Node IDs.
		let ips: BTreeMap<N, String> =
			serde_json::from_str(&options.mining.hbbft_validator_ip_addresses).unwrap();
		assert!(keygen.public_keys().keys().eq(ips.keys()));
	}

	#[test]
	fn test_network_info_serde() {
		let num_nodes = 1;
		let mut rng = rand::thread_rng();
		let enodes_map = generate_enodes(num_nodes, None);

		let pub_keys = enodes_to_pub_keys(&enodes_map);
		let (sync_keygen, _, _) = generate_keygens(pub_keys, &mut rng, (num_nodes - 1) / 3);

		let keygen = sync_keygen.iter().nth(0).unwrap();
		let toml_string = toml::to_string(&to_toml(
			keygen,
			&enodes_map,
			1,
			&ConfigType::PosdaoSetup,
			None,
			&Address::default(),
		))
		.unwrap();
		let config: TomlHbbftOptions = toml::from_str(&toml_string).unwrap();
		compare(keygen, &config);
	}

	#[test]
	fn test_threshold_encryption_single() {
		let (secret, public, _) = crate::create_account();
		let keypair = KeyPairWrapper { public, secret };
		let mut pub_keys: BTreeMap<Public, KeyPairWrapper> = BTreeMap::new();
		pub_keys.insert(public, keypair.clone());
		let mut rng = rand::thread_rng();
		let mut key_gen =
			SyncKeyGen::new(public, keypair, Arc::new(pub_keys), 0, &mut rng).unwrap();
		let part = key_gen.1.unwrap();
		let outcome = key_gen.0.handle_part(&public, part, &mut rng);
		assert!(outcome.is_ok());
		match outcome.unwrap() {
			PartOutcome::Valid(ack) => {
				assert!(ack.is_some());
				let ack_outcome = key_gen.0.handle_ack(&public, ack.unwrap());
				assert!(ack_outcome.is_ok());
				match ack_outcome.unwrap() {
					AckOutcome::Valid => {
						assert!(key_gen.0.is_ready());
						let key_shares = key_gen.0.generate();
						assert!(key_shares.is_ok());
						assert!(key_shares.unwrap().1.is_some());
					}
					AckOutcome::Invalid(_) => assert!(false),
				}
			}
			PartOutcome::Invalid(_) => assert!(false),
		}
	}

	#[test]
	fn test_threshold_encryption_multiple() {
		let num_nodes = 4;
		let t = 1;

		let enodes = generate_enodes(num_nodes, None);
		let pub_keys = enodes_to_pub_keys(&enodes);
		let mut rng = rand::thread_rng();

		let (sync_keygen, _, _) = generate_keygens(pub_keys, &mut rng, t);

		let compare_to = sync_keygen.iter().nth(0).unwrap().generate().unwrap().0;

		// Check key generation
		for s in sync_keygen {
			assert!(s.is_ready());
			assert!(s.generate().is_ok());
			assert_eq!(s.generate().unwrap().0, compare_to);
		}
	}
}
