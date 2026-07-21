use crate::actions::maven::error::Error;

// version requirement attached to a library entry in maven.yml
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constraint {
	Eq(String),
	Ge(String),
	Le(String),
	Latest,
}

// groupId:artifactId, without a version, used as the identity for conflict resolution
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Coordinate {
	pub group_id: String,
	pub artifact_id: String,
}

impl Coordinate {
	pub fn key(&self) -> String {
		format!("{}:{}", self.group_id, self.artifact_id)
	}
}

impl std::fmt::Display for Coordinate {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}:{}", self.group_id, self.artifact_id)
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResolvedCoordinate {
	pub group_id: String,
	pub artifact_id: String,
	pub version: String,
}

impl ResolvedCoordinate {
	pub fn coordinate(&self) -> Coordinate {
		Coordinate { group_id: self.group_id.clone(), artifact_id: self.artifact_id.clone() }
	}

	// groupId/artifactId
	pub fn group_path(&self) -> String {
		self.group_id.replace('.', "/")
	}
}

impl std::fmt::Display for ResolvedCoordinate {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}:{}:{}", self.group_id, self.artifact_id, self.version)
	}
}

pub fn parse_library_entry(raw: &str) -> Result<(Coordinate, Constraint), Error> {
	let raw = raw.trim();
	let mut parts = raw.splitn(3, ':');

	let group_id = parts.next().filter(|s| !s.is_empty()).ok_or_else(|| Error::InvalidCoordinate(raw.to_string()))?;
	let artifact_id = parts.next().filter(|s| !s.is_empty()).ok_or_else(|| Error::InvalidCoordinate(raw.to_string()))?;
	let coordinate = Coordinate { group_id: group_id.to_string(), artifact_id: artifact_id.to_string() };

	let constraint = match parts.next() {
		None => Constraint::Latest,
		Some(raw_constraint) => parse_constraint(raw_constraint, raw)?,
	};

	Ok((coordinate, constraint))
}

fn parse_constraint(raw_constraint: &str, full_entry: &str) -> Result<Constraint, Error> {
	if let Some(version) = raw_constraint.strip_prefix("==") {
		return non_empty_version(version, full_entry).map(|v| Constraint::Eq(v.to_string()));
	}
	if let Some(version) = raw_constraint.strip_prefix(">=") {
		return non_empty_version(version, full_entry).map(|v| Constraint::Ge(v.to_string()));
	}
	if let Some(version) = raw_constraint.strip_prefix("<=") {
		return non_empty_version(version, full_entry).map(|v| Constraint::Le(v.to_string()));
	}
	Err(Error::InvalidCoordinate(full_entry.to_string()))
}

fn non_empty_version<'a>(version: &'a str, full_entry: &str) -> Result<&'a str, Error> {
	if version.is_empty() {
		return Err(Error::InvalidCoordinate(full_entry.to_string()));
	}
	Ok(version)
}

// parses "com.example.lib:lib:1.2.3" style entries from the trusted list, always an exact version
pub fn parse_trusted_entry(raw: &str) -> Result<ResolvedCoordinate, Error> {
	let raw = raw.trim();
	let mut parts = raw.splitn(3, ':');

	let group_id = parts.next().filter(|s| !s.is_empty()).ok_or_else(|| Error::InvalidCoordinate(raw.to_string()))?;
	let artifact_id = parts.next().filter(|s| !s.is_empty()).ok_or_else(|| Error::InvalidCoordinate(raw.to_string()))?;
	let version = parts.next().filter(|s| !s.is_empty()).ok_or_else(|| Error::InvalidCoordinate(raw.to_string()))?;

	Ok(ResolvedCoordinate { group_id: group_id.to_string(), artifact_id: artifact_id.to_string(), version: version.to_string() })
}

// dot separated numeric version compare, non numeric segments compare as 0 (best effort, mirrors build.rs compiler version compare)
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
	let parse = |v: &str| -> Vec<u64> { v.split(['.', '-']).map(|part| part.parse::<u64>().unwrap_or(0)).collect() };
	let left = parse(a);
	let right = parse(b);
	let len = left.len().max(right.len());
	for i in 0..len {
		let l = left.get(i).copied().unwrap_or(0);
		let r = right.get(i).copied().unwrap_or(0);
		let ordering = l.cmp(&r);
		if ordering != std::cmp::Ordering::Equal {
			return ordering;
		}
	}
	std::cmp::Ordering::Equal
}

pub fn unwrap_version_range(raw: &str) -> String {
	let trimmed = raw.trim();
	let is_range = trimmed.starts_with(['[', '(']) && trimmed.ends_with([']', ')']);
	if !is_range {
		return trimmed.to_string();
	}
	let inner = &trimmed[1..trimmed.len() - 1];
	let lower = inner.split(',').next().unwrap_or("").trim();
	if lower.is_empty() {
		return trimmed.to_string();
	}
	lower.to_string()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parses_latest_when_no_constraint_given() {
		let (coordinate, constraint) = parse_library_entry("com.example.lib:lib").unwrap();
		assert_eq!(coordinate.group_id, "com.example.lib");
		assert_eq!(coordinate.artifact_id, "lib");
		assert_eq!(constraint, Constraint::Latest);
	}

	#[test]
	fn parses_eq_constraint() {
		let (_, constraint) = parse_library_entry("com.example.lib:lib:==1.2.3").unwrap();
		assert_eq!(constraint, Constraint::Eq("1.2.3".to_string()));
	}

	#[test]
	fn parses_ge_and_le_constraints() {
		let (_, ge) = parse_library_entry("g:a:>=1.0.0").unwrap();
		assert_eq!(ge, Constraint::Ge("1.0.0".to_string()));
		let (_, le) = parse_library_entry("g:a:<=1.0.0").unwrap();
		assert_eq!(le, Constraint::Le("1.0.0".to_string()));
	}

	#[test]
	fn rejects_missing_group_or_artifact() {
		assert!(parse_library_entry("com.example.lib").is_err());
		assert!(parse_library_entry(":lib").is_err());
	}

	#[test]
	fn version_compare_orders_numerically_not_lexically() {
		assert_eq!(compare_versions("1.9.0", "1.10.0"), std::cmp::Ordering::Less);
	}
}
