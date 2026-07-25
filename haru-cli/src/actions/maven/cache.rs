use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::actions::maven::coordinate::ResolvedCoordinate;
use crate::actions::maven::error::Error;
use crate::actions::maven::hash::Algorithm;

pub const CACHE_DIR: &str = "build/cache/maven";
const INDEX_PATH: &str = "build/cache/maven/index.json";
const ARTIFACTS_DIR: &str = "build/cache/maven/artifacts";
const POM_INDEX_PATH: &str = "build/cache/maven/pom_index.json";
const POMS_DIR: &str = "build/cache/maven/poms";

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Index {
	// key: "groupId:artifactId:version", value: cached artifact record
	#[serde(default)]
	entries: HashMap<String, IndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
	pub source: String,
	pub file_path: String,
	pub packaging: String,
	pub checksum_algorithm: String,
	pub checksum_hex: String,
}

impl Index {
	pub fn load() -> Result<Self, Error> {
		if !Path::new(INDEX_PATH).exists() {
			return Ok(Self::default());
		}
		let contents = std::fs::read_to_string(INDEX_PATH)?;
		let index = serde_json::from_str(&contents).unwrap_or_default();
		Ok(index)
	}

	pub fn save(&self) -> Result<(), Error> {
		std::fs::create_dir_all(CACHE_DIR)?;
		let contents = serde_json::to_string_pretty(self).map_err(|err| Error::Io(std::io::Error::other(err)))?;
		std::fs::write(INDEX_PATH, contents)?;
		Ok(())
	}

	pub fn get(&self, resolved: &ResolvedCoordinate) -> Option<&IndexEntry> {
		self.entries.get(&resolved.to_string())
	}

	pub fn insert(&mut self, resolved: &ResolvedCoordinate, entry: IndexEntry) {
		self.entries.insert(resolved.to_string(), entry);
	}

	// every cached version for a groupId:artifactId, regardless of which version is keyed
	pub fn versions_for(&self, coordinate: &crate::actions::maven::coordinate::Coordinate) -> Vec<String> {
		let prefix = format!("{}:", coordinate.key());
		self.entries.keys().filter_map(|key| key.strip_prefix(&prefix)).map(str::to_string).collect()
	}
}

pub fn artifact_cache_path(resolved: &ResolvedCoordinate, extension: &str) -> PathBuf {
	Path::new(ARTIFACTS_DIR)
		.join(resolved.group_path())
		.join(&resolved.artifact_id)
		.join(&resolved.version)
		.join(format!("{}-{}.{extension}", resolved.artifact_id, resolved.version))
}

pub fn is_entry_valid(entry: &IndexEntry) -> bool {
	let Ok(bytes) = std::fs::read(&entry.file_path) else {
		return false;
	};
	let Some(algorithm) = parse_algorithm(&entry.checksum_algorithm) else {
		return false;
	};
	crate::actions::maven::hash::digest_hex(algorithm, &bytes) == entry.checksum_hex
}

fn parse_algorithm(raw: &str) -> Option<Algorithm> {
	match raw {
		"sha256" => Some(Algorithm::Sha256),
		"sha1" => Some(Algorithm::Sha1),
		"md5" => Some(Algorithm::Md5),
		_ => None,
	}
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PomIndex {
	// key: "groupId:artifactId:version", value: cached pom record
	#[serde(default)]
	entries: HashMap<String, PomIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PomIndexEntry {
	pub source: String,
	pub file_path: String,
}

impl PomIndex {
	pub fn load() -> Result<Self, Error> {
		if !Path::new(POM_INDEX_PATH).exists() {
			return Ok(Self::default());
		}
		let contents = std::fs::read_to_string(POM_INDEX_PATH)?;
		let index = serde_json::from_str(&contents).unwrap_or_default();
		Ok(index)
	}

	pub fn save(&self) -> Result<(), Error> {
		std::fs::create_dir_all(CACHE_DIR)?;
		let contents = serde_json::to_string_pretty(self).map_err(|err| Error::Io(std::io::Error::other(err)))?;
		std::fs::write(POM_INDEX_PATH, contents)?;
		Ok(())
	}

	pub fn get_xml(&self, resolved: &ResolvedCoordinate) -> Option<(String, String)> {
		let entry = self.entries.get(&resolved.to_string())?;
		let xml = std::fs::read_to_string(&entry.file_path).ok()?;
		Some((xml, entry.source.clone()))
	}

	pub fn insert(&mut self, resolved: &ResolvedCoordinate, entry: PomIndexEntry) {
		self.entries.insert(resolved.to_string(), entry);
	}
}

pub fn pom_cache_path(resolved: &ResolvedCoordinate) -> PathBuf {
	Path::new(POMS_DIR).join(resolved.group_path()).join(&resolved.artifact_id).join(&resolved.version).join(format!("{}-{}.pom", resolved.artifact_id, resolved.version))
}
