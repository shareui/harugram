use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
	Sha256,
	Sha1,
	Md5,
}

impl Algorithm {
	pub const FALLBACK_ORDER: [Self; 3] = [Self::Sha256, Self::Sha1, Self::Md5];
	
	pub fn extension(self) -> &'static str {
		match self {
			Self::Sha256 => "sha256",
			Self::Sha1 => "sha1",
			Self::Md5 => "md5",
		}
	}
}

pub fn digest_hex(algorithm: Algorithm, bytes: &[u8]) -> String {
	match algorithm {
		Algorithm::Sha256 => hex(Sha256::digest(bytes).as_slice()),
		Algorithm::Sha1 => hex(Sha1::digest(bytes).as_slice()),
		Algorithm::Md5 => hex(Md5::digest(bytes).as_slice()),
	}
}

fn hex(bytes: &[u8]) -> String {
	bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn extract_hex_token(raw: &str) -> Option<String> {
	let token = raw.split_whitespace().next()?;
	let looks_like_hex = !token.is_empty() && token.chars().all(|c| c.is_ascii_hexdigit());
	looks_like_hex.then(|| token.to_lowercase())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn digest_hex_matches_known_sha256() {
		let hash = digest_hex(Algorithm::Sha256, b"hello world");
		assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde");
	}

	#[test]
	fn extract_hex_token_ignores_trailing_filename() {
		let extracted = extract_hex_token("d41d8cd98f00b204e9800998ecf8427e  filename.jar\n");
		assert_eq!(extracted.as_deref(), Some("d41d8cd98f00b204e9800998ecf8427e"));
	}

	#[test]
	fn extract_hex_token_rejects_non_hex() {
		assert_eq!(extract_hex_token("<html>not found</html>"), None);
	}
}
