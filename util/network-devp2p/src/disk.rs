use log::*;
use parity_crypto::publickey::Secret;
use parity_path::restrict_permissions_owner;
use std::fmt::Debug;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub trait DiskEntity: FromStr {
	const PATH: &'static str;
	const DESC: &'static str;

	fn to_repr(&self) -> String;
}

impl DiskEntity for Secret {
	const PATH: &'static str = "key";
	const DESC: &'static str = "key file";

	fn to_repr(&self) -> String {
		self.to_hex()
	}
}

pub fn save<E: DiskEntity>(path: &Path, entity: &E) {
	let mut path_buf = PathBuf::from(path);
	if let Err(e) = fs::create_dir_all(path_buf.as_path()) {
		warn!("Error creating {} directory: {:?}", E::DESC, e);
		return;
	};
	path_buf.push(E::PATH);
	let path = path_buf.as_path();
	let mut file = match fs::File::create(&path) {
		Ok(file) => file,
		Err(e) => {
			warn!("Error creating {}: {:?}", E::DESC, e);
			return;
		}
	};
	if let Err(e) = restrict_permissions_owner(path, true, false) {
		warn!(target: "network", "Failed to modify permissions of the file ({})", e);
	}
	if let Err(e) = file.write(&entity.to_repr().into_bytes()) {
		warn!("Error writing {}: {:?}", E::DESC, e);
	}
}

pub fn load<E>(path: &Path) -> Option<E>
where
	E: DiskEntity,
	<E as FromStr>::Err: Debug,
{
	let mut path_buf = PathBuf::from(path);
	path_buf.push(E::PATH);
	let mut file = match fs::File::open(path_buf.as_path()) {
		Ok(file) => file,
		Err(e) => {
			debug!("Error opening {}: {:?}", E::DESC, e);
			return None;
		}
	};
	let mut buf = String::new();
	match file.read_to_string(&mut buf) {
		Ok(_) => {},
		Err(e) => {
			warn!("Error reading {}: {:?}", E::DESC, e);
			return None;
		}
	}
	match E::from_str(&buf) {
		Ok(key) => Some(key),
		Err(e) => {
			warn!("Error parsing {}: {:?}", E::DESC, e);
			None
		}
	}
}

#[cfg(test)]
mod tests {
	#[test]
	fn key_save_load() {
		use super::*;
		use ethereum_types::H256;
		use tempdir::TempDir;

		let tempdir = TempDir::new("").unwrap();
		let key = Secret::from(H256::random());
		save(tempdir.path(), &key);
		let r = load(tempdir.path());
		assert_eq!(key, r.unwrap());
	}
}