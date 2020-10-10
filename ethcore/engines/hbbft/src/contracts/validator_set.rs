use client_traits::EngineClient;
use common_types::ids::BlockId;
use ethereum_types::Address;
use parity_crypto::publickey::Public;
use std::collections::BTreeMap;
use std::str::FromStr;
use utils::bound_contract::{BoundContract, CallError};

use_contract!(validator_set_hbbft, "res/validator_set_hbbft.json");

lazy_static! {
	static ref VALIDATOR_SET_ADDRESS: Address =
		Address::from_str("1000000000000000000000000000000000000001").unwrap();
}

macro_rules! call_const_validator {
	($c:ident, $x:ident $(, $a:expr )*) => {
		$c.call_const(validator_set_hbbft::functions::$x::call($($a),*))
	};
}

pub fn get_validator_pubkeys(
	client: &dyn EngineClient,
) -> Result<BTreeMap<Address, Public>, CallError> {
	let c = BoundContract::bind(client, BlockId::Latest, *VALIDATOR_SET_ADDRESS);
	let validators = call_const_validator!(c, get_validators)?;
	let mut validator_map = BTreeMap::new();
	for v in validators {
		let pubkey = call_const_validator!(c, get_public_key, v)?;

		if pubkey.len() != 64 {
			return Err(CallError::ReturnValueInvalid);
		}
		let pubkey = Public::from_slice(&pubkey);

		println!("Validator {:?} with public key {}", v, pubkey);
		validator_map.insert(v, pubkey);
	}
	Ok(validator_map)
}
