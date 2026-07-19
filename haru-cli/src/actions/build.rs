use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

const HARU_YML: &str = "haru.yml";
const KOTLIN_MAIN_EXT: &str = "kt";
const JAVA_MAIN_EXT: &str = "java";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
	Sdk,
	Extension,
}

#[derive(Debug)]
pub enum Error {
	HaruYmlNotFound,
	HaruYmlInvalid(String),
	UnknownTarget(String),
	CompilerNotFound(&'static str),
	CompilerVersionMismatch { compiler: &'static str, constraint: String, found: String },
	VersionConstraintInvalid { compiler: &'static str, raw: String },
	LibNotFound(String),
	SourceNotFound(String),
	MainFileNotFound(String),
	ClassPackageMismatch { file: String, expected: String, found: String },
	MetadataYmlNotFound(String),
	MetadataYmlInvalid(String),
	MetadataFieldMissing(&'static str),
	MetadataFieldTypeMismatch { field: String, expected: &'static str, got: &'static str },
	Io(std::io::Error),
}

// TODO: add the theme colour
// TODO: add a progressbar
impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::HaruYmlNotFound => write!(f, "haru.yml not found"),
			Self::HaruYmlInvalid(reason) => write!(f, "haru.yml is invalid: {reason}"),
			Self::UnknownTarget(target) => write!(f, "unknown target \"{target}\" in haru.yml"),
			Self::CompilerNotFound(compiler) => write!(f, "{compiler} not found"),
			Self::CompilerVersionMismatch { compiler, constraint, found } => {
				write!(f, "{compiler} version {found} does not satisfy {constraint}")
			}
			Self::VersionConstraintInvalid { compiler, raw } => write!(f, "invalid version constraint for {compiler}: \"{raw}\""),
			Self::LibNotFound(path) => write!(f, "lib not found: {path}"),
			Self::SourceNotFound(path) => write!(f, "source not found: {path}"),
			Self::MainFileNotFound(class_name) => write!(f, "file not found: {class_name}.kt / {class_name}.java"),
			Self::ClassPackageMismatch { file, expected, found } => {
				write!(f, "package mismatch in {file}: expected \"{expected}\", found \"{found}\"")
			}
			Self::MetadataYmlNotFound(path) => write!(f, "metadata file not found: {path}"),
			Self::MetadataYmlInvalid(reason) => write!(f, "metadata.yml is invalid: {reason}"),
			Self::MetadataFieldMissing(field) => write!(f, "metadata.yml is missing required field \"{field}\""),
			Self::MetadataFieldTypeMismatch { field, expected, got } => {
				write!(f, "field \"{field}\" in metadata.yml expected type {expected}, got {got}")
			}
			Self::Io(err) => write!(f, "{err}"),
		}
	}
}

pub fn run(verbose: bool) -> Result<(), Error> {
	let haru_yml = read_haru_yml()?;
	let target = read_target(&haru_yml, verbose)?;

	check_compilers(&haru_yml, verbose)?;
	check_libs(&haru_yml, verbose)?;
	let source_path = check_source(&haru_yml, verbose)?;
	check_class_matches_package(&haru_yml, &source_path, verbose)?;
	let metadata_path = check_metadata_exists(&haru_yml, verbose)?;

	if target == Target::Sdk {
		let metadata = read_metadata_yml(&metadata_path)?;
		check_metadata_required_fields(&metadata, verbose)?;
		check_metadata_types(&metadata, verbose)?;
	}

	Ok(())
}

fn log(verbose: bool, message: &str) {
	if verbose {
		println!("{message}");
	}
}

fn read_haru_yml() -> Result<Value, Error> {
	if !Path::new(HARU_YML).exists() {
		return Err(Error::HaruYmlNotFound);
	}
	let contents = fs::read_to_string(HARU_YML).map_err(Error::Io)?;
	serde_saphyr::from_str::<Value>(&contents).map_err(|err| Error::HaruYmlInvalid(err.to_string()))
}

fn read_target(haru_yml: &Value, verbose: bool) -> Result<Target, Error> {
	let raw = haru_yml.get("target").and_then(Value::as_str).ok_or_else(|| Error::HaruYmlInvalid("missing field \"target\"".to_string()))?;
	let target = match raw {
		"sdk" => Target::Sdk,
		"extension" => Target::Extension,
		other => return Err(Error::UnknownTarget(other.to_string())),
	};
	log(verbose, &format!("Target found: {raw}"));
	Ok(target)
}

// step 1: kotlinc/javac presence and version constraint, whichever of the two is configured
fn check_compilers(haru_yml: &Value, verbose: bool) -> Result<(), Error> {
	if let Some(constraint) = haru_yml.get("kotlinc").and_then(Value::as_str) {
		check_compiler_version("kotlinc", constraint, verbose)?;
	}
	if let Some(constraint) = haru_yml.get("javac").and_then(Value::as_str) {
		check_compiler_version("javac", constraint, verbose)?;
	}
	Ok(())
}

fn check_compiler_version(compiler: &'static str, constraint: &str, verbose: bool) -> Result<(), Error> {
	let (operator, required) = parse_constraint(compiler, constraint)?;

	let path = which(compiler).map_err(Error::Io)?;
	if path.is_none() {
		return Err(Error::CompilerNotFound(compiler));
	}

	let found = compiler_version(compiler).map_err(Error::Io)?;
	let Some(found) = found else {
		return Err(Error::CompilerNotFound(compiler));
	};

	let found_parts = parse_version(&found);
	let satisfies = match operator {
		Operator::Eq => compare_versions(&found_parts, &required) == std::cmp::Ordering::Equal,
		Operator::Ge => compare_versions(&found_parts, &required) != std::cmp::Ordering::Less,
		Operator::Le => compare_versions(&found_parts, &required) != std::cmp::Ordering::Greater,
	};

	if !satisfies {
		return Err(Error::CompilerVersionMismatch { compiler, constraint: constraint.to_string(), found });
	}
	log(verbose, &format!("Compiler {compiler} found, version {found}"));
	Ok(())
}

#[derive(Debug, Clone, Copy)]
enum Operator {
	Eq,
	Ge,
	Le,
}

// >= <= == / error
fn parse_constraint(compiler: &'static str, raw: &str) -> Result<(Operator, Vec<u64>), Error> {
	let invalid = || Error::VersionConstraintInvalid { compiler, raw: raw.to_string() };

	let (operator, version_str) = if let Some(rest) = raw.strip_prefix("==") {
		(Operator::Eq, rest)
	} else if let Some(rest) = raw.strip_prefix(">=") {
		(Operator::Ge, rest)
	} else if let Some(rest) = raw.strip_prefix("<=") {
		(Operator::Le, rest)
	} else {
		return Err(invalid());
	};

	let version = parse_version(version_str);
	if version.is_empty() {
		return Err(invalid());
	}
	Ok((operator, version))
}

fn parse_version(raw: &str) -> Vec<u64> {
	raw.split('.').filter_map(|part| part.parse::<u64>().ok()).collect()
}

fn compare_versions(left: &[u64], right: &[u64]) -> std::cmp::Ordering {
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

fn which(binary: &str) -> std::io::Result<Option<std::path::PathBuf>> {
	let finder = if cfg!(windows) { "where" } else { "which" };
	let output = Command::new(finder).arg(binary).output()?;
	if !output.status.success() {
		return Ok(None);
	}
	let stdout = String::from_utf8_lossy(&output.stdout);
	let first_line = stdout.lines().next().unwrap_or("").trim();
	if first_line.is_empty() {
		return Ok(None);
	}
	Ok(Some(std::path::PathBuf::from(first_line)))
}

fn compiler_version(binary: &str) -> std::io::Result<Option<String>> {
	let output = Command::new(binary).arg("-version").output()?;
	let combined = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
	Ok(extract_version(&combined))
}

fn extract_version(text: &str) -> Option<String> {
	for word in text.split_whitespace() {
		let cleaned = word.trim_matches(|c: char| !c.is_ascii_digit() && c != '.');
		let looks_like_version = cleaned.chars().next().is_some_and(|c| c.is_ascii_digit()) && cleaned.contains('.');
		if looks_like_version {
			return Some(cleaned.to_string());
		}
	}
	None
}

// step 2: every path under libs must exist
fn check_libs(haru_yml: &Value, verbose: bool) -> Result<(), Error> {
	let Some(libs) = haru_yml.get("libs").and_then(Value::as_array) else {
		return Ok(());
	};
	for lib in libs {
		let Some(path) = lib.as_str() else {
			continue;
		};
		if !Path::new(path).exists() {
			return Err(Error::LibNotFound(path.to_string()));
		}
		log(verbose, &format!("Library {path} found"));
	}
	Ok(())
}

// step 3: source directory must exist
fn check_source(haru_yml: &Value, verbose: bool) -> Result<String, Error> {
	let source = haru_yml.get("source").and_then(Value::as_str).unwrap_or("src").to_string(); // TODO
	if !Path::new(&source).exists() {
		return Err(Error::SourceNotFound(source));
	}
	log(verbose, "Source directory found");
	Ok(source)
}

// step 4: find the main file
fn check_class_matches_package(haru_yml: &Value, source_path: &str, verbose: bool) -> Result<(), Error> {
	let Some(class) = haru_yml.get("class").and_then(Value::as_str) else {
		return Err(Error::HaruYmlInvalid("missing field \"class\"".to_string()));
	};
	let Some((expected_package, class_name)) = class.rsplit_once('.') else {
		return Ok(());
	};

	for ext in [KOTLIN_MAIN_EXT, JAVA_MAIN_EXT] {
		let path = Path::new(source_path).join(format!("{class_name}.{ext}"));
		if !path.exists() {
			continue;
		}
		let Some(found_package) = read_pkg_dec(&path).map_err(Error::Io)? else {
			continue;
		};
		if found_package != expected_package {
			return Err(Error::ClassPackageMismatch {
				file: path.display().to_string(),
				expected: expected_package.to_string(),
				found: found_package,
			});
		}
		log(verbose, &format!("{class} class was successfully found"));
		return Ok(());
	}

	Err(Error::MainFileNotFound(class_name.to_string()))
}

fn read_pkg_dec (path: &Path) -> std::io::Result<Option<String>> {
	let contents = fs::read_to_string(path)?;
	for line in contents.lines() {
		let trimmed = line.trim();
		let Some(rest) = trimmed.strip_prefix("package ") else {
			continue;
		};
		let name = rest.trim().trim_end_matches(';').trim();
		return Ok(Some(name.to_string()));
	}
	Ok(None)
}

// step 5: metadata.yml
fn check_metadata_exists(haru_yml: &Value, verbose: bool) -> Result<String, Error> {
	let path = haru_yml.get("metadata").and_then(Value::as_str).unwrap_or("metadata.yml").to_string();
	if !Path::new(&path).exists() {
		return Err(Error::MetadataYmlNotFound(path));
	}
	log(verbose, "Metadata found");
	Ok(path)
}

fn read_metadata_yml(path: &str) -> Result<Value, Error> {
	let contents = fs::read_to_string(path).map_err(Error::Io)?;
	serde_saphyr::from_str::<Value>(&contents).map_err(|err| Error::MetadataYmlInvalid(err.to_string()))
}

// step 6 (sdk only): id, version, author
fn check_metadata_required_fields(metadata: &Value, verbose: bool) -> Result<(), Error> {
	for field in ["id", "version", "author"] {
		if metadata.get(field).is_none() {
			return Err(Error::MetadataFieldMissing(field));
		}
		log(verbose, &format!("{field} found"));
	}
	Ok(())
}

const KNOWN_METADATA_STRING_FIELDS: [&str; 6] = ["id", "version", "state", "author", "app_version", "source"];

// step 7 (sdk only): types ==
fn check_metadata_types(metadata: &Value, verbose: bool) -> Result<(), Error> {
	let Value::Object(map) = metadata else {
		return Err(Error::MetadataYmlInvalid("root must be a mapping".to_string()));
	};

	for (field, value) in map {
		if field == "socials" {
			check_socials_are_strings(value)?;
			log(verbose, &format!("{field} matches the type"));
			continue;
		}
		if KNOWN_METADATA_STRING_FIELDS.contains(&field.as_str()) {
			check_is_string(field, value)?;
			log(verbose, &format!("{field} matches the type"));
			continue;
		}
		log(verbose, &format!("Unknown field, skipping: {field}"));
	}
	Ok(())
}

// TODO


fn check_socials_are_strings(value: &Value) -> Result<(), Error> {
	let Some(entries) = value.as_array() else {
		return check_is_string("socials", value);
	};
	for (i, entry) in entries.iter().enumerate() {
		check_is_string(&format!("socials[{i}]"), entry)?;
	}
	Ok(())
}

fn check_is_string(field: &str, value: &Value) -> Result<(), Error> {
	if value.is_string() {
		return Ok(());
	}
	Err(Error::MetadataFieldTypeMismatch { field: field.to_string(), expected: "string", got: json_type_name(value) })
}

fn json_type_name(value: &Value) -> &'static str {
	match value {
		Value::Null => "null",
		Value::Bool(_) => "bool",
		Value::Number(number) if number.is_i64() || number.is_u64() => "int",
		Value::Number(_) => "float",
		Value::String(_) => "string",
		Value::Array(_) => "array",
		Value::Object(_) => "object",
	}
}
