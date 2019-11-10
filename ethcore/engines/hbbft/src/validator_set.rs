use client_traits::BlockChainClient;
use common_types::ids::BlockId;
use ethabi::FunctionOutputDecoder;
use ethereum_types::Address;
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

macro_rules! eval_contract_result {
	($x:ident, $c:ident) => {
		{
			let (data, decoder) = validator_set_hbbft_mock::functions::$x::call();
			let return_data = match $c.call_contract(BlockId::Latest, *VALIDATOR_SET_ADDRESS, data) {
				Ok(val) => val,
				Err(_) => {
					error!(target: "engine", "Failed to read validators from validator set contract.");
					return;
				}
			};
			if return_data.is_empty() {
				error!(target: "engine", "The call to get the current set of validators returned no data.");
				return;
			} else {
				decoder.decode(&return_data).unwrap()
			}
		}
	};
}

fn get_validator_map(client: &dyn BlockChainClient) {
	let validators = eval_contract_result!(get_validators, client);
	//let validator_map = BTreeMap::new();
	for v in validators {
		//let (pubkey_high, pubkey_low) = eval_contract_result!(validator_pubkeys, client);
		// Add validator-associated data to the map
		//map.insert(v, );
	}
}
