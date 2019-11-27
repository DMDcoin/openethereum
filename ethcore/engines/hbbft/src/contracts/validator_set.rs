use client_traits::EngineClient;
use common_types::ids::BlockId;
use ethereum_types::Address;
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

pub fn get_validators(client: &dyn EngineClient) -> Result<Vec<Address>, CallError> {
	let c = BoundContract::bind(client, BlockId::Latest, *VALIDATOR_SET_ADDRESS);
	Ok(call_const_validator!(c, get_validators)?)
}

pub fn staking_by_mining_address(
	client: &dyn EngineClient,
	mining_address: Address,
) -> Result<Address, CallError> {
	let c = BoundContract::bind(client, BlockId::Latest, *VALIDATOR_SET_ADDRESS);
	Ok(call_const_validator!(
		c,
		staking_by_mining_address,
		mining_address
	)?)
}
