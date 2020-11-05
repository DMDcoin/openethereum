#[cfg(test)]
pub mod tests {
	use client_traits::EngineClient;
	use common_types::ids::BlockId;
	use ethereum_types::{Address, U256};
	use parity_crypto::publickey::Public;
	use std::str::FromStr;
	use utils::bound_contract::{BoundContract, CallError};

	use_contract!(staking_contract, "res/staking_contract.json");

	lazy_static! {
		static ref STAKING_CONTRACT_ADDRESS: Address =
			Address::from_str("1100000000000000000000000000000000000001").unwrap();
	}

	macro_rules! call_const_staking {
	($c:ident, $x:ident $(, $a:expr )*) => {
		$c.call_const(staking_contract::functions::$x::call($($a),*))
	};
}

	pub fn min_staking(client: &dyn EngineClient) -> Result<U256, CallError> {
		let c = BoundContract::bind(client, BlockId::Latest, *STAKING_CONTRACT_ADDRESS);
		call_const_staking!(c, candidate_min_stake)
	}

	pub fn add_pool(mining_address: Address, mining_public_key: Public) -> ethabi::Bytes {
		let (abi_bytes, _) = staking_contract::functions::add_pool::call(
			mining_address,
			mining_public_key.as_bytes(),
			[0; 16],
		);
		abi_bytes
	}
}
