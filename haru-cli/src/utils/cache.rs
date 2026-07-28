// uses bithash for a hashes
use std::fs;
use std::path::{Path, PathBuf};

use crate::utils::bithash;

const FINGERPRINT_FILE: &str = "fingerprint";
const MANIFEST_FILE: &str = "manifest";

// accumulates a fingerprint over file contents and plain metadata (paths, flags, args)
pub struct Fingerprint {
	hasher: bithash::Hasher,
}

impl Fingerprint {
	pub fn new() -> Self {
		Self { hasher: bithash::Hasher::new(bithash::SEED_DEFAULT) }
	}

	pub fn add_file(&mut self, path: &Path) -> std::io::Result<()> {
		self.add_str(&path.to_string_lossy());
		let contents = fs::read(path)?;
		self.hasher.update(&contents);
		Ok(())
	}

	// mixes in plain text
	pub fn add_str(&mut self, value: &str) {
		self.hasher.update(value.as_bytes());
		self.hasher.update(&[0u8]); // separator so "a","b" and "ab"
	}

	pub fn finish(self) -> u64 {
		self.hasher.finish()
	}
}

pub fn add_dir(fingerprint: &mut Fingerprint, dir: &Path) -> std::io::Result<()> {
	if !dir.is_dir() {
		return Ok(());
	}
	let mut entries: Vec<PathBuf> = fs::read_dir(dir)?.filter_map(|entry| entry.ok()).map(|entry| entry.path()).collect();
	entries.sort();

	for path in entries {
		if is_own_marker(&path) {
			continue;
		}
		if path.is_dir() {
			add_dir(fingerprint, &path)?;
		} else {
			fingerprint.add_file(&path)?;
		}
	}
	Ok(())
}

fn is_own_marker(path: &Path) -> bool {
	matches!(path.file_name().and_then(|name| name.to_str()), Some(FINGERPRINT_FILE) | Some(MANIFEST_FILE))
}

// fingerprint
fn marker_path(cache_dir: &Path) -> PathBuf {
	cache_dir.join(FINGERPRINT_FILE)
}

pub fn is_fresh(cache_dir: &Path, hash: u64) -> bool {
	let Ok(stored) = fs::read_to_string(marker_path(cache_dir)) else {
		return false;
	};
	stored.trim() == format!("{hash:016x}")
}

pub fn store(cache_dir: &Path, hash: u64) -> std::io::Result<()> {
	fs::create_dir_all(cache_dir)?;
	fs::write(marker_path(cache_dir), format!("{hash:016x}"))
}

fn manifest_path(cache_dir: &Path) -> PathBuf {
	cache_dir.join(MANIFEST_FILE)
}

pub fn store_manifest(cache_dir: &Path, paths: &[PathBuf]) -> std::io::Result<()> {
	let joined = paths.iter().map(|path| path.to_string_lossy().into_owned()).collect::<Vec<_>>().join("\n");
	fs::write(manifest_path(cache_dir), joined)
}

pub fn load_manifest(cache_dir: &Path) -> std::io::Result<Vec<PathBuf>> {
	let contents = fs::read_to_string(manifest_path(cache_dir))?;
	Ok(contents.lines().filter(|line| !line.is_empty()).map(PathBuf::from).collect())
}
