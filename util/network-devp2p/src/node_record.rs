use log::*;
use parity_crypto::publickey::Secret;
use crate::{disk::DiskEntity, node_table::NodeEndpoint};

pub type Enr = enr::Enr<secp256k1::SecretKey>;

pub struct EnrManager {
	secret: secp256k1::SecretKey,
	inner: Enr,
}

#[allow(dead_code)]
impl EnrManager {
    pub fn new(key: Secret, seq: u64) -> Option<Self> {
		let secret = key.to_secp256k1_secret().ok()?;
		let mut b = enr::EnrBuilder::new("v4");
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
