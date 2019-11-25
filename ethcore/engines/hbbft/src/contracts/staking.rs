use ethereum_types::Address;
use std::str::FromStr;

use_contract!(
	staking_hbbft,
	"res/staking_contract.json"
);

lazy_static! {
	static ref STAKING_ADDRESS: Address =
	    // Is this the correct address, or is it '0x1100000000000000000000000000000000000001'?
		Address::from_str("1100000000000000000000000000000000000000").unwrap();
}
