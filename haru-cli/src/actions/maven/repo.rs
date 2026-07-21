use crate::actions::maven::coordinate::ResolvedCoordinate;
use crate::actions::maven::error::Error;
use crate::actions::maven::hash::Algorithm;

const READ_LIMIT_BYTES: u64 = 200 * 1024 * 1024;

// {source}/{group/path}/{artifact}/{version}/
pub fn artifact_dir_url(source: &str, coordinate: &ResolvedCoordinate) -> String {
	let source = source.trim_end_matches('/');
	format!("{source}/{}/{}/{}/", coordinate.group_path(), coordinate.artifact_id, coordinate.version)
}

pub fn artifact_file_url(source: &str, coordinate: &ResolvedCoordinate, classifier: Option<&str>, extension: &str) -> String {
	let dir = artifact_dir_url(source, coordinate);
	let suffix = classifier.map(|c| format!("-{c}")).unwrap_or_default();
	format!("{dir}{}-{}{suffix}.{extension}", coordinate.artifact_id, coordinate.version)
}

pub fn metadata_url(source: &str, group_id: &str, artifact_id: &str) -> String {
	let source = source.trim_end_matches('/');
	format!("{source}/{}/{artifact_id}/maven-metadata.xml", group_id.replace('.', "/"))
}

pub fn fetch_text(url: &str) -> Result<Option<String>, Error> {
	match ureq::get(url).call() {
		Ok(mut response) => {
			let body = response
				.body_mut()
				.with_config()
				.limit(READ_LIMIT_BYTES)
				.read_to_string()
				.map_err(|err| Error::Network { url: url.to_string(), reason: err.to_string() })?;
			Ok(Some(body))
		}
		Err(ureq::Error::StatusCode(404)) => Ok(None),
		Err(err) => Err(Error::Network { url: url.to_string(), reason: err.to_string() }),
	}
}

pub fn fetch_bytes(url: &str) -> Result<Option<Vec<u8>>, Error> {
	match ureq::get(url).call() {
		Ok(mut response) => {
			let body = response
				.body_mut()
				.with_config()
				.limit(READ_LIMIT_BYTES)
				.read_to_vec()
				.map_err(|err| Error::Network { url: url.to_string(), reason: err.to_string() })?;
			Ok(Some(body))
		}
		Err(ureq::Error::StatusCode(404)) => Ok(None),
		Err(err) => Err(Error::Network { url: url.to_string(), reason: err.to_string() }),
	}
}

pub fn fetch_checksum(artifact_url: &str) -> Result<Option<(Algorithm, String)>, Error> {
	for algorithm in Algorithm::FALLBACK_ORDER {
		let checksum_url = format!("{artifact_url}.{}", algorithm.extension());
		let Some(raw) = fetch_text(&checksum_url)? else {
			continue;
		};
		if let Some(hex) = crate::actions::maven::hash::extract_hex_token(&raw) {
			return Ok(Some((algorithm, hex)));
		}
	}
	Ok(None)
}
