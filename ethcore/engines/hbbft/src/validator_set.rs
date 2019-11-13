use client_traits::EngineClient;
use common_types::ids::BlockId;
use ethereum_types::Address;
use ethkey::Public;
use std::collections::BTreeMap;
use std::str::FromStr;

use util::CallError;

use_contract!(
	validator_set_hbbft_mock,
	"res/validator_set_hbbft_mock.json"
);

lazy_static! {
	static ref VALIDATOR_SET_ADDRESS: Address =
		Address::from_str("9000000000000000000000000000000000000000").unwrap();
}

macro_rules! call_const_validator {
	($c:ident, $x:ident $(, $a:expr ),*) => {
		$c.call_const(validator_set_hbbft_mock::functions::$x::call($($a),*))
	};
}

pub fn get_validator_map(
	client: &dyn EngineClient,
) -> Result<BTreeMap<Address, Public>, CallError> {
	// bind contract
	let c = crate::util::BoundContract::bind(client, BlockId::Latest, *VALIDATOR_SET_ADDRESS);

	let validators = call_const_validator!(c, get_validators)?;

	let mut validator_map = BTreeMap::new();
	for v in validators {
		let (pubkey_high, pubkey_low) = call_const_validator!(c, validator_pubkeys, v)?;
		let pubkey = Public::from_slice(&[pubkey_high.as_bytes(), pubkey_low.as_bytes()].concat());
		validator_map.insert(v, pubkey);
	}
	Ok(validator_map)
}
