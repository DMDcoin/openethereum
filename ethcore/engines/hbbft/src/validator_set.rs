use client_traits::BlockChainClient;
use common_types::ids::BlockId;
use ethabi::FunctionOutputDecoder;
use ethereum_types::Address;
use ethkey::Public;
use std::collections::BTreeMap;
use std::str::FromStr;

use_contract!(
	validator_set_hbbft_mock,
	"res/validator_set_hbbft_mock.json"
);

lazy_static! {
	static ref VALIDATOR_SET_ADDRESS: Address =
		Address::from_str("9000000000000000000000000000000000000000").unwrap();
}

/// Contract call failed error.
#[derive(Debug)]
pub enum CallError {
	/// The call itself failed.
	CallFailed(String),
	/// Decoding the return value failed or the decoded value was a failure.
	DecodeFailed(ethabi::Error),
}

macro_rules! eval_contract_impl {
	($i:ident, $x:ident, $c:ident $(, $a:expr ),*) => {
		{
			let (data, decoder) = $i::functions::$x::call($($a),*);
			let return_data = $c.call_contract(BlockId::Latest, *VALIDATOR_SET_ADDRESS, data)
				.map_err(CallError::CallFailed)?;
			decoder.decode(&return_data).map_err(CallError::DecodeFailed)?
		}
	};
}

macro_rules! eval_validator_set {
	($x:ident, $c:ident $(, $a:expr ),*) => {
		eval_contract_impl!(validator_set_hbbft_mock, $x, $c $(,$a),*)
	};
}

pub fn get_validator_map(
	client: &dyn BlockChainClient,
) -> Result<BTreeMap<Address, Public>, CallError> {
	let validators = eval_validator_set!(get_validators, client);
	let mut validator_map = BTreeMap::new();
	for v in validators {
		let (pubkey_high, pubkey_low) = eval_validator_set!(validator_pubkeys, client, v);
		let pubkey = Public::from_slice(&[pubkey_high.as_bytes(), pubkey_low.as_bytes()].concat());
		validator_map.insert(v, pubkey);
	}
	Ok(validator_map)
}
