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

pub enum ValidatorType {
	Current,
	Pending,
}

pub fn get_validator_pubkeys(
	client: &dyn EngineClient,
	block_id: BlockId,
	validator_type: ValidatorType,
) -> Result<BTreeMap<Address, Public>, CallError> {
	let c = BoundContract::bind(client, block_id, *VALIDATOR_SET_ADDRESS);
	let validators = match validator_type {
		ValidatorType::Current => call_const_validator!(c, get_validators)?,
		ValidatorType::Pending => call_const_validator!(c, get_pending_validators)?,
	};
	let mut validator_map = BTreeMap::new();
	for v in validators {
		let pubkey = call_const_validator!(c, get_public_key, v)?;

		if pubkey.len() != 64 {
			return Err(CallError::ReturnValueInvalid);
		}
		let pubkey = Public::from_slice(&pubkey);

		//println!("Validator {:?} with public key {}", v, pubkey);
		validator_map.insert(v, pubkey);
	}
	Ok(validator_map)
}

#[cfg(test)]
pub fn mining_by_staking_address(
	client: &dyn EngineClient,
	staking_address: &Address,
) -> Result<Address, CallError> {
	let c = BoundContract::bind(client, BlockId::Latest, *VALIDATOR_SET_ADDRESS);
	call_const_validator!(c, mining_by_staking_address, staking_address.clone())
}

// pub fn staking_by_mining_address(
// 	client: &dyn EngineClient,
// 	mining_address: &Address,
// ) -> Result<Address, CallError> {
// 	let c = BoundContract::bind(client, BlockId::Latest, *VALIDATOR_SET_ADDRESS);
// 	call_const_validator!(c, staking_by_mining_address, mining_address.clone())
// }

pub fn is_pending_validator(
	client: &dyn EngineClient,
	staking_address: &Address,
) -> Result<bool, CallError> {
	let c = BoundContract::bind(client, BlockId::Latest, *VALIDATOR_SET_ADDRESS);
	call_const_validator!(c, is_pending_validator, staking_address.clone())
}

pub fn get_pending_validators(client: &dyn EngineClient) -> Result<Vec<Address>, CallError> {
	let c = BoundContract::bind(client, BlockId::Latest, *VALIDATOR_SET_ADDRESS);
	call_const_validator!(c, get_pending_validators)
}
