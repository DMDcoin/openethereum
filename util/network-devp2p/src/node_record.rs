// Copyright 2015-2020 Parity Technologies (UK) Ltd.
// This file is part of OpenEthereum.

// OpenEthereum is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// OpenEthereum is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Open Ethereum.  If not, see <http://www.gnu.org/licenses/>.

use log::*;
use parity_crypto::publickey::Secret;
use crate::{disk::DiskEntity, node_table::NodeEndpoint};

pub type Enr = enr::Enr<secp256k1::SecretKey>;

const ENR_VERSION: &str = "v4";

pub struct EnrManager {
	secret: secp256k1::SecretKey,
	inner: Enr,
}

#[allow(dead_code)]
impl EnrManager {
    pub fn new(key: Secret, seq: u64) -> Option<Self> {
		let secret = key.to_secp256k1_secret().ok()?;
		let mut b = enr::EnrBuilder::new(ENR_VERSION);
		b.seq(seq);
		let inner = b.build(&secret).ok()?;
		Some(Self { secret, inner })
	}

	pub fn load(key: Secret, inner: Enr) -> Option<Self> {
		let secret = key.to_secp256k1_secret().ok()?;
		let public = secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &secret);

		if inner.public_key() != public {
			warn!("ENR does not match the provided key");
			return None;
		}
		Some(Self { secret, inner })
	}

	pub fn with_node_endpoint(mut self, endpoint: &NodeEndpoint) -> Self {
		self.set_node_endpoint(endpoint);
		self
	}

	pub fn set_node_endpoint(&mut self, endpoint: &NodeEndpoint) {
		let seq = self.inner.seq();
		self.inner.set_tcp_socket(endpoint.address, &self.secret).expect("Not enough data to go over the limit; qed");
		self.inner.set_udp(endpoint.udp_port, &self.secret).expect("Not enough data to go over the limit; qed");
		// TODO: what if we overflow here? Reset the node private key? That would require force crashing the client?
		self.inner.set_seq(seq + 1, &self.secret).unwrap();
	}

	pub fn as_enr(&self) -> &Enr {
		&self.inner
	}

	pub fn into_enr(self) -> Enr {
		self.inner
	}
}

impl DiskEntity for Enr {
	const PATH: &'static str = "enr";
	const DESC: &'static str = "Ethereum Node Record";

	fn to_repr(&self) -> String {
		self.to_base64()
	}
}
