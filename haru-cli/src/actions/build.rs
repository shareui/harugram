use std::fs;
use std::path::Path;
use std::process::Command;

use owo_colors::OwoColorize;
use serde_json::Value;

use crate::actions::compile;
use crate::actions::lib_prompt;
use crate::actions::package::{self, Password};
use crate::progress::Logger;

const HARU_YML: &str = "haru.yml";
const KOTLIN_MAIN_EXT: &str = "kt";
const JAVA_MAIN_EXT: &str = "java";
const CACHE_DIR: &str = "build/cache/";
const BASE_STEPS: u32 = 7;
const COMPILE_STEPS: u32 = 1;
const SDK_STEPS: u32 = 2;
const PACKAGE_STEPS: u32 = 1;
// same color print_found (lib_prompt.rs) uses for a successfully resolved absolute path
const BRIGHT_GREEN: (u8, u8, u8) = (0, 255, 0);

// flags accepted by the build command
pub struct BuildOptions {
	pub verbose: bool,
	pub release: bool,
	pub compression: Option<u8>,
	pub password: Option<Vec<String>>,
}

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
	ApkNotFound(String),
	SdkOnlyFlag(&'static str),
	Compile(compile::Error),
	Package(package::Error),
	Io(std::io::Error),
}

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
			Self::ApkNotFound(path) => write!(f, "stub not found: {path}"),
			Self::SdkOnlyFlag(flag) => write!(f, "flag \"{flag}\" can only be used when target is \"sdk\""),
			Self::Compile(err) => write!(f, "{err}"),
			Self::Package(err) => write!(f, "{err}"),
			Self::Io(err) => write!(f, "{err}"),
		}
	}
}

impl Error {
	// extra gray hint line shown below the error, currently only set for missing compile tools
	pub fn hint(&self) -> Option<String> {
		match self {
			Self::Compile(err) => err.hint(),
			_ => None,
		}
	}
}

pub fn run(options: BuildOptions) -> Result<(), Error> {
	let haru_yml = read_haru_yml()?;
	let (target, target_line) = read_target(&haru_yml)?;

	if target != Target::Sdk && options.compression.is_some() {
		return Err(Error::SdkOnlyFlag("-c/--compression"));
	}
	if target != Target::Sdk && options.password.is_some() {
		return Err(Error::SdkOnlyFlag("-p/--password"));
	}

	let total_steps = if target == Target::Sdk { BASE_STEPS + SDK_STEPS + COMPILE_STEPS + PACKAGE_STEPS } else { BASE_STEPS + COMPILE_STEPS };
	let mut logger = Logger::new(options.verbose, total_steps);
	logger.log(&target_line);

	let result = run_checks(&haru_yml, target, &options, &mut logger);
	logger.finish();
	result
}

fn run_checks(haru_yml: &Value, target: Target, options: &BuildOptions, logger: &mut Logger) -> Result<(), Error> {
	check_compilers(haru_yml, logger)?;
	logger.step();
	check_libs(haru_yml, logger)?;
	logger.step();
	let source_path = check_source(haru_yml, logger)?;
	logger.step();
	check_class_matches_package(haru_yml, &source_path, logger)?;
	logger.step();
	let metadata_path = check_metadata_exists(haru_yml, logger)?;
	logger.step();

	let mut sdk_id = String::new();
	if target == Target::Sdk {
		let metadata = read_metadata_yml(&metadata_path)?;
		check_metadata_required_fields(&metadata, logger)?;
		logger.step();
		check_metadata_types(&metadata, logger)?;
		logger.step();
		sdk_id = metadata.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
	}

	check_stubs(haru_yml, logger)?;
	logger.step();
	ensure_cache_dir(logger)?;
	logger.step();

	compile::run(haru_yml, &source_path, options.release, logger).map_err(Error::Compile)?;

	if target == Target::Sdk {
		let password = options.password.as_ref().map(|pair| Password { algorithm: pair[0].clone(), value: pair[1].clone() });
		let output_path = package::run(&metadata_path, options.compression, password.as_ref(), options.release).map_err(Error::Package)?;
		let sdk_name = if options.release { "release.harusdk" } else { "debug.harusdk" };
		logger.log(&format!("Packaged {sdk_name}"));
		logger.step();
		let (gr, gg, gb) = BRIGHT_GREEN;
		logger.print_always(&format!("You have successfully assembled {sdk_id}, congratulations!").truecolor(gr, gg, gb).to_string());
		logger.print_always(&format!("The result can be found in this path: {}", output_path.display()).truecolor(gr, gg, gb).to_string());
	}

	Ok(())
}

fn read_haru_yml() -> Result<Value, Error> {
	if !Path::new(HARU_YML).exists() {
		return Err(Error::HaruYmlNotFound);
	}
	let contents = fs::read_to_string(HARU_YML).map_err(Error::Io)?;
	serde_saphyr::from_str::<Value>(&contents).map_err(|err| Error::HaruYmlInvalid(err.to_string()))
}

fn read_target(haru_yml: &Value) -> Result<(Target, String), Error> {
	let raw = haru_yml.get("target").and_then(Value::as_str).ok_or_else(|| Error::HaruYmlInvalid("missing field \"target\"".to_string()))?;
	let target = match raw {
		"sdk" => Target::Sdk,
		"extension" => Target::Extension,
		other => return Err(Error::UnknownTarget(other.to_string())),
	};
	Ok((target, format!("Target found: {raw}")))
}

// step 1: kotlinc/javac presence and version constraint, whichever of the two is configured
fn check_compilers(haru_yml: &Value, logger: &mut Logger) -> Result<(), Error> {
	if let Some(constraint) = haru_yml.get("kotlinc").and_then(Value::as_str) {
		check_compiler_version("kotlinc", constraint, logger)?;
	}
	if let Some(constraint) = haru_yml.get("javac").and_then(Value::as_str) {
		check_compiler_version("javac", constraint, logger)?;
	}
	Ok(())
}

fn check_compiler_version(compiler: &'static str, constraint: &str, logger: &mut Logger) -> Result<(), Error> {
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
	logger.log(&format!("Compiler {compiler} found, version {found}"));
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

// step 2: every path under libs must exist, offers an interactive system search if missing
fn check_libs(haru_yml: &Value, logger: &mut Logger) -> Result<(), Error> {
	let Some(libs) = haru_yml.get("static-libs").and_then(Value::as_array) else {
		return Ok(());
	};
	for lib in libs {
		let Some(path) = lib.as_str() else {
			continue;
		};
		if !Path::new(path).exists() {
			match lib_prompt::resolve_missing(logger, lib_prompt::Kind::Library, path) {
				lib_prompt::Resolution::Resolved => {}
				lib_prompt::Resolution::Aborted => return Err(Error::LibNotFound(path.to_string())),
			}
		}
		logger.log(&format!("Library {path} found"));
	}
	Ok(())
}

// step 3: source directory must exist
fn check_source(haru_yml: &Value, logger: &mut Logger) -> Result<String, Error> {
	let source = haru_yml.get("source").and_then(Value::as_str).unwrap_or("src").to_string(); // TODO
	if !Path::new(&source).exists() {
		return Err(Error::SourceNotFound(source));
	}
	logger.log("Source directory found");
	Ok(source)
}

// step 4: find the main file
fn check_class_matches_package(haru_yml: &Value, source_path: &str, logger: &mut Logger) -> Result<(), Error> {
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
		logger.log(&format!("{class} class was successfully found"));
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
fn check_metadata_exists(haru_yml: &Value, logger: &mut Logger) -> Result<String, Error> {
	let path = haru_yml.get("metadata").and_then(Value::as_str).unwrap_or("metadata.yml").to_string();
	if !Path::new(&path).exists() {
		return Err(Error::MetadataYmlNotFound(path));
	}
	logger.log("Metadata found");
	Ok(path)
}

fn read_metadata_yml(path: &str) -> Result<Value, Error> {
	let contents = fs::read_to_string(path).map_err(Error::Io)?;
	serde_saphyr::from_str::<Value>(&contents).map_err(|err| Error::MetadataYmlInvalid(err.to_string()))
}

// step 6 (sdk only): id, version, author
fn check_metadata_required_fields(metadata: &Value, logger: &mut Logger) -> Result<(), Error> {
	for field in ["id", "version", "author"] {
		if metadata.get(field).is_none() {
			return Err(Error::MetadataFieldMissing(field));
		}
		logger.log(&format!("{field} found"));
	}
	Ok(())
}

const KNOWN_METADATA_STRING_FIELDS: [&str; 6] = ["id", "version", "state", "author", "app_version", "source"];

// step 7 (sdk only): types ==
fn check_metadata_types(metadata: &Value, logger: &mut Logger) -> Result<(), Error> {
	let Value::Object(map) = metadata else {
		return Err(Error::MetadataYmlInvalid("root must be a mapping".to_string()));
	};

	for (field, value) in map {
		if field == "socials" {
			check_socials_are_strings(value)?;
			logger.log(&format!("{field} matches the type"));
			continue;
		}
		if KNOWN_METADATA_STRING_FIELDS.contains(&field.as_str()) {
			check_is_string(field, value)?;
			logger.log(&format!("{field} matches the type"));
			continue;
		}
		logger.log(&format!("Unknown field, skipping: {field}"));
	}
	Ok(())
}

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

// step 8: every path under stubs must exist, offers an interactive system search if missing
fn check_stubs(haru_yml: &Value, logger: &mut Logger) -> Result<(), Error> {
	let Some(stubs) = haru_yml.get("stubs").and_then(Value::as_array) else {
		return Ok(());
	};
	for stub in stubs {
		let Some(path) = stub.as_str() else {
			continue;
		};
		if !Path::new(path).exists() {
			match lib_prompt::resolve_missing(logger, lib_prompt::Kind::Stub, path) {
				lib_prompt::Resolution::Resolved => {}
				lib_prompt::Resolution::Aborted => return Err(Error::ApkNotFound(path.to_string())),
			}
		}
		logger.log(&format!("Stub found: {path}"));
	}
	Ok(())
}

// step 9: build/cache/ must exist, created if missing
fn ensure_cache_dir(logger: &mut Logger) -> Result<(), Error> {
	let cache_dir = Path::new(CACHE_DIR);
	if cache_dir.exists() {
		logger.log("Cache directory found");
		return Ok(());
	}
	fs::create_dir_all(cache_dir).map_err(Error::Io)?;
	logger.log("Cache directory created");
	Ok(())
}
