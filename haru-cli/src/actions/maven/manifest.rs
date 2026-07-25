use std::path::Path;

use serde_json::Value;

use crate::actions::maven::coordinate::{self, Coordinate, Constraint, ResolvedCoordinate};
use crate::actions::maven::error::Error;

pub const MAVEN_YML: &str = "maven.yml";

pub struct Manifest {
	pub sources: Vec<String>,
	pub transit: bool,
	pub check_across_all_repos: bool,
	pub libraries: Vec<(Coordinate, Constraint)>,
	pub trust_system: bool,
	pub trusted: Vec<ResolvedCoordinate>,
}

pub fn load() -> Result<Option<Manifest>, Error> {
	if !Path::new(MAVEN_YML).exists() {
		return Ok(None);
	}
	let contents = std::fs::read_to_string(MAVEN_YML)?;
	let root = serde_saphyr::from_str::<Value>(&contents).map_err(|err| Error::MavenYmlInvalid(err.to_string()))?;

	let sources = read_string_array(&root, "sources");
	let transit = root.get("transit").and_then(Value::as_bool).unwrap_or(true);
	let check_across_all_repos = root.get("checkAcrossAllRepos").and_then(Value::as_bool).unwrap_or(false);
	let trust_system = root.get("trustSystem").and_then(Value::as_bool).unwrap_or(false);

	let libraries = read_string_array(&root, "libraries")
		.iter()
		.map(|raw| coordinate::parse_library_entry(raw))
		.collect::<Result<Vec<_>, _>>()?;

	let trusted = read_string_array(&root, "trusted")
		.iter()
		.filter(|raw| raw.as_str() != "None")
		.map(|raw| coordinate::parse_trusted_entry(raw))
		.collect::<Result<Vec<_>, _>>()?;

	Ok(Some(Manifest { sources, transit, check_across_all_repos, libraries, trust_system, trusted }))
}

fn read_string_array(root: &Value, field: &str) -> Vec<String> {
	let Some(array) = root.get(field).and_then(Value::as_array) else {
		return Vec::new();
	};
	array.iter().filter_map(Value::as_str).map(str::to_string).collect()
}

pub fn add_trusted(resolved: &ResolvedCoordinate) -> Result<(), Error> {
	let contents = std::fs::read_to_string(MAVEN_YML)?;
	let entry_line = format!("    - {resolved}");

	let updated = if let Some(replaced) = replace_none_placeholder(&contents, &entry_line) {
		replaced
	} else {
		append_to_trusted_list(&contents, &entry_line)
	};

	std::fs::write(MAVEN_YML, updated)?;
	Ok(())
}

fn replace_none_placeholder(contents: &str, entry_line: &str) -> Option<String> {
	let mut lines: Vec<&str> = contents.lines().collect();
	let placeholder_index = lines.iter().position(|line| line.trim() == "- None")?;
	let owned_entry = entry_line.to_string();
	lines[placeholder_index] = &owned_entry;
	let joined = lines.join("\n");
	Some(with_trailing_newline(joined))
}

fn append_to_trusted_list(contents: &str, entry_line: &str) -> String {
	let lines: Vec<&str> = contents.lines().collect();
	let Some(trusted_index) = lines.iter().position(|line| line.trim_end() == "trusted:") else {
		// no trusted: list yet, create one at the end
		let mut joined = contents.trim_end().to_string();
		joined.push_str("\ntrusted:\n");
		joined.push_str(entry_line);
		return with_trailing_newline(joined);
	};

	let insert_at = trusted_index + 1;
	let mut new_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
	new_lines.insert(insert_at, entry_line.to_string());
	with_trailing_newline(new_lines.join("\n"))
}

fn with_trailing_newline(mut text: String) -> String {
	if !text.ends_with('\n') {
		text.push('\n');
	}
	text
}
