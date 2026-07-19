use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use zip::write::FileOptions;
use zip::{AesMode, CompressionMethod, ZipWriter};

const OUTPUT_DIR: &str = "build/output";
const DEBUG_SDK_NAME: &str = "debug.harusdk";
const RELEASE_SDK_NAME: &str = "release.harusdk";
const CLASSES_DEX: &str = "build/classes.dex";
const HARU_YML: &str = "haru.yml";
const MIN_COMPRESSION_LEVEL: u8 = 0;
const MAX_COMPRESSION_LEVEL: u8 = 9;

#[derive(Debug)]
pub enum Error {
	SourceMissing(String),
	InvalidCompressionLevel(u8),
	InvalidPasswordAlgorithm(String),
	Zip(zip::result::ZipError),
	Io(std::io::Error),
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::SourceMissing(path) => write!(f, "cannot package: {path} not found"),
			Self::InvalidCompressionLevel(level) => {
				write!(f, "invalid compression level {level}, expected {MIN_COMPRESSION_LEVEL}-{MAX_COMPRESSION_LEVEL}")
			}
			Self::InvalidPasswordAlgorithm(raw) => write!(f, "unknown encryption algorithm \"{raw}\", expected aes128, aes192 or aes256"),
			Self::Zip(err) => write!(f, "packaging error: {err}"),
			Self::Io(err) => write!(f, "{err}"),
		}
	}
}

// password encryption requested through -p/--password
pub struct Password {
	pub algorithm: String,
	pub value: String,
}

// normalizes an algorithm string: trim, lowercase, strip everything but letters and digits
fn normalize_algorithm(raw: &str) -> String {
	raw.trim().to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

fn resolve_aes_mode(raw: &str) -> Result<AesMode, Error> {
	match normalize_algorithm(raw).as_str() {
		"aes128" => Ok(AesMode::Aes128),
		"aes192" => Ok(AesMode::Aes192),
		"aes256" => Ok(AesMode::Aes256),
		_ => Err(Error::InvalidPasswordAlgorithm(raw.to_string())),
	}
}

pub fn run(metadata_path: &str, compression: Option<u8>, password: Option<&Password>, release: bool) -> Result<PathBuf, Error> {
	for path in [CLASSES_DEX, metadata_path, HARU_YML] {
		if !Path::new(path).exists() {
			return Err(Error::SourceMissing(path.to_string()));
		}
	}

	let options = build_options(compression, password)?;

	fs::create_dir_all(OUTPUT_DIR).map_err(Error::Io)?;
	let sdk_name = if release { RELEASE_SDK_NAME } else { DEBUG_SDK_NAME };
	let output_path = Path::new(OUTPUT_DIR).join(sdk_name);
	if output_path.exists() {
		fs::remove_file(&output_path).map_err(Error::Io)?;
	}

	let file = File::create(&output_path).map_err(Error::Io)?;
	let mut writer = ZipWriter::new(file);

	write_entry(&mut writer, CLASSES_DEX, "classes.dex", options)?;
	write_entry(&mut writer, metadata_path, "metadata.yml", options)?;
	write_entry(&mut writer, HARU_YML, "haru.yml", options)?;

	writer.finish().map_err(Error::Zip)?;
	output_path.canonicalize().map_err(Error::Io)
}

fn build_options<'k>(compression: Option<u8>, password: Option<&'k Password>) -> Result<FileOptions<'k, ()>, Error> {
	let mut options: FileOptions<'k, ()> = FileOptions::default();

	options = match compression {
		Some(0) => options.compression_method(CompressionMethod::Stored),
		Some(level @ 1..=MAX_COMPRESSION_LEVEL) => options.compression_method(CompressionMethod::Deflated).compression_level(Some(level.into())),
		Some(level) => return Err(Error::InvalidCompressionLevel(level)),
		None => options.compression_method(CompressionMethod::Deflated),
	};

	if let Some(password) = password {
		let mode = resolve_aes_mode(&password.algorithm)?;
		options = options.with_aes_encryption(mode, &password.value);
	}

	Ok(options)
}

fn write_entry<'k>(writer: &mut ZipWriter<File>, source_path: &str, entry_name: &str, options: FileOptions<'k, ()>) -> Result<(), Error> {
	let contents = fs::read(source_path).map_err(Error::Io)?;
	writer.start_file(entry_name, options).map_err(Error::Zip)?;
	writer.write_all(&contents).map_err(Error::Io)?;
	Ok(())
}
