use serde::Deserialize;

use crate::actions::maven::error::Error;

#[derive(Debug, Deserialize)]
#[serde(rename = "metadata")]
struct MavenMetadata {
	versioning: Versioning,
}

#[derive(Debug, Deserialize)]
struct Versioning {
	release: Option<String>,
	latest: Option<String>,
	versions: Option<Versions>,
}

#[derive(Debug, Deserialize)]
struct Versions {
	#[serde(rename = "version", default)]
	version: Vec<String>,
}

pub fn parse_release_version(xml: &str, coordinate_label: &str) -> Result<Option<String>, Error> {
	let metadata: MavenMetadata =
		quick_xml::de::from_str(xml).map_err(|err| Error::MetadataInvalid { coordinate: coordinate_label.to_string(), reason: err.to_string() })?;

	if let Some(release) = metadata.versioning.release.filter(|v| !v.is_empty()) {
		return Ok(Some(release));
	}
	if let Some(latest) = metadata.versioning.latest.filter(|v| !v.is_empty()) {
		return Ok(Some(latest));
	}

	let versions = metadata.versioning.versions.map(|v| v.version).unwrap_or_default();
	let highest = versions.into_iter().max_by(|a, b| crate::actions::maven::coordinate::compare_versions(a, b));
	Ok(highest)
}

pub fn parse_all_versions(xml: &str, coordinate_label: &str) -> Result<Vec<String>, Error> {
	let metadata: MavenMetadata =
		quick_xml::de::from_str(xml).map_err(|err| Error::MetadataInvalid { coordinate: coordinate_label.to_string(), reason: err.to_string() })?;
	Ok(metadata.versioning.versions.map(|v| v.version).unwrap_or_default())
}

#[cfg(test)]
mod tests {
	use super::*;

	const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>androidx.core</groupId>
  <artifactId>core</artifactId>
  <versioning>
    <latest>1.13.0</latest>
    <release>1.13.0</release>
    <versions>
      <version>1.0.0</version>
      <version>1.12.0</version>
      <version>1.13.0</version>
    </versions>
    <lastUpdated>20240101000000</lastUpdated>
  </versioning>
</metadata>"#;

	#[test]
	fn picks_release_version() {
		let version = parse_release_version(SAMPLE, "androidx.core:core").unwrap();
		assert_eq!(version.as_deref(), Some("1.13.0"));
	}

	#[test]
	fn lists_all_versions() {
		let versions = parse_all_versions(SAMPLE, "androidx.core:core").unwrap();
		assert_eq!(versions, vec!["1.0.0", "1.12.0", "1.13.0"]);
	}
}
