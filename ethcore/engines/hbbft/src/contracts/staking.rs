use client_traits::EngineClient;
use common_types::ids::BlockId;
use ethereum_types::Address;
use parity_crypto::publickey::Public;
use std::str::FromStr;
use utils::bound_contract::{BoundContract, CallError};

use_contract!(staking_hbbft, "res/staking_contract.json");

lazy_static! {
	static ref STAKING_ADDRESS: Address =
		Address::from_str("1100000000000000000000000000000000000001").unwrap();
}

macro_rules! call_const_staking {
	($c:ident, $x:ident $(, $a:expr )*) => {
		$c.call_const(staking_hbbft::functions::$x::call($($a),*))
	};
}

pub fn get_pool_pubkey(client: &dyn EngineClient, address: Address) -> Result<Public, CallError> {
	let c = BoundContract::bind(client, BlockId::Latest, *STAKING_ADDRESS);
	let pub_key = call_const_staking!(c, get_pool_public_key, address)?;
	if pub_key.len() != 64 {
		return Err(CallError::ReturnValueInvalid);
	}
	let pub_key = Public::from_slice(&pub_key);
	// println!("Public Key for staking address {}: {:?}", address, pub_key);
	Ok(pub_key)
}
