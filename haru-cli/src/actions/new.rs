use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{Receiver, Sender};

use crate::actions::kotlin_stdlib;
use crate::tui::sdk_creating::state::{Language, NewProjectState};

const DEFAULT_SDK_ID: &str = "com.example.mysdk";
const DEFAULT_APP_VERSION: &str = ">=0.8.6";
const DEFAULT_SOURCE: &str = "github.com/username/my-sdk";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
	FindCompiler,
	FindKotlinStdlib,
	CreateSrcDir,
	CreateMetadata,
	CreateHaruYml,
	CreateMainFile,
}

impl Step {
	pub const ORDER: [Self; 6] =
		[Self::FindCompiler, Self::FindKotlinStdlib, Self::CreateSrcDir, Self::CreateMetadata, Self::CreateHaruYml, Self::CreateMainFile];
}

pub enum Event {
	Log(String),
	Warn(String),
	Confirm(String),
	SkipAvailable(bool),
	StepDone,
	Finished(Result<(), Error>),
}

#[derive(Debug)]
pub enum Error {
	AlreadyExists(String),
	Io(std::io::Error),
	Terminated,
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::AlreadyExists(path) => write!(f, "File {path} already exists!"),
			Self::Io(err) => write!(f, "{err}"),
			Self::Terminated => write!(f, "Terminated by user"),
		}
	}
}

// snapshot of the form
pub struct Request {
	pub sdk_id: String,
	pub author: String,
	pub app_version: String,
	pub source: Option<String>,
	pub kotlin_stdlib: bool,
	pub use_current_compiler: bool,
	pub language: Language,
}

impl Request {
	pub fn from_state(state: &NewProjectState) -> Self {
		let source = if state.open_source { Some(non_empty(&state.source.value, DEFAULT_SOURCE)) } else { None };

		Self {
			sdk_id: non_empty(&state.sdk_id.value, DEFAULT_SDK_ID),
			author: state.author.value.clone(),
			app_version: non_empty(&state.app_version.value, DEFAULT_APP_VERSION),
			source,
			kotlin_stdlib: state.kotlin_stdlib,
			use_current_compiler: state.use_current_compiler,
			language: state.language,
		}
	}
}

fn non_empty(value: &str, placeholder: &str) -> String {
	if value.is_empty() { placeholder.to_string() } else { value.to_string() }
}

#[derive(Default)]
struct Created {
	src_dir: bool,
	metadata_yml: bool,
	haru_yml: bool,
	main_file: Option<&'static str>,
}

impl Created {
	fn cleanup(&self) {
		if let Some(path) = self.main_file {
			let _ = fs::remove_file(path);
		}
		if self.haru_yml {
			let _ = fs::remove_file("haru.yml");
		}
		if self.metadata_yml {
			let _ = fs::remove_file("metadata.yml");
		}
		if self.src_dir {
			let _ = fs::remove_dir_all("src");
		}
	}
}

pub fn run(request: Request, tx: Sender<Event>, confirm_rx: Receiver<bool>, skip_rx: Receiver<()>) {
	let mut created = Created::default();
	let result = run_steps(&request, &tx, &confirm_rx, &skip_rx, &mut created);
	if result.is_err() {
		created.cleanup();
	}
	let _ = tx.send(Event::Finished(result));
}

fn run_steps(request: &Request, tx: &Sender<Event>, confirm_rx: &Receiver<bool>, skip_rx: &Receiver<()>, created: &mut Created) -> Result<(), Error> {
	check_no_conflicts()?;

	let (compiler_version, compiler_found) = find_compiler_version(request, tx, confirm_rx)?;
	step_done(tx);

	let kotlin_stdlib_path = find_kotlin_stdlib_path(request, compiler_found, tx, confirm_rx, skip_rx)?;
	step_done(tx);

	fs::create_dir("src").map_err(Error::Io)?;
	created.src_dir = true;
	log(tx, "Creating src/".to_string());
	step_done(tx);

	write_metadata_yml(request, tx)?;
	created.metadata_yml = true;
	step_done(tx);

	write_haru_yml(request, &compiler_version, kotlin_stdlib_path.as_deref(), tx)?;
	created.haru_yml = true;
	step_done(tx);

	let main_path = write_main_file(request, tx)?;
	created.main_file = Some(main_path);
	step_done(tx);

	Ok(())
}

fn check_no_conflicts() -> Result<(), Error> {
	for path in ["haru.yml", "metadata.yml", "src"] {
		if Path::new(path).exists() {
			return Err(Error::AlreadyExists(path.to_string()));
		}
	}
	Ok(())
}

fn log(tx: &Sender<Event>, message: String) {
	let _ = tx.send(Event::Log(message));
}

fn warn(tx: &Sender<Event>, message: String) {
	let _ = tx.send(Event::Warn(message));
}

fn step_done(tx: &Sender<Event>) {
	let _ = tx.send(Event::StepDone);
}

// sends a warning
fn confirm_or_terminate(tx: &Sender<Event>, confirm_rx: &Receiver<bool>, message: String) -> Result<(), Error> {
	warn(tx, message);
	let _ = tx.send(Event::Confirm("Continue? [Y/n]: ".to_string()));
	let answered_yes = confirm_rx.recv().unwrap_or(false);
	if answered_yes {
		return Ok(());
	}
	log(tx, "Termination...".to_string());
	Err(Error::Terminated)
}

fn compiler_binary(language: Language) -> &'static str {
	match language {
		Language::Kotlin => "kotlinc",
		Language::Java => "javac",
	}
}

fn find_compiler_version(request: &Request, tx: &Sender<Event>, confirm_rx: &Receiver<bool>) -> Result<(String, bool), Error> {
	let compiler = compiler_binary(request.language);

	if !request.use_current_compiler {
		log(tx, "Skip compiler version search".to_string());
		return Ok(("any".to_string(), true));
	}

	log(tx, format!("Finding the {compiler} version"));

	let path = match which(compiler) {
		Ok(Some(path)) => path,
		Ok(None) => {
			confirm_or_terminate(tx, confirm_rx, format!("Warn: {compiler} not found!"))?;
			return Ok(("any # not found".to_string(), false));
		}
		Err(e) => {
			confirm_or_terminate(tx, confirm_rx, format!("Warn: Error finding {compiler}: {e}"))?;
			return Ok(("any # not found".to_string(), false));
		}
	};
	log(tx, format!("Found {compiler} at {}", path.display()));

	let version = match compiler_version(compiler) {
		Ok(Some(version)) => version,
		Ok(None) => {
			confirm_or_terminate(tx, confirm_rx, format!("Warn: Invalid {compiler} version"))?;
			return Ok(("any # not found".to_string(), true));
		}
		Err(e) => {
			confirm_or_terminate(tx, confirm_rx, format!("Warn: Error finding {compiler}: {e}"))?;
			return Ok(("any # not found".to_string(), true));
		}
	};

	log(tx, format!("Version {compiler} found: {version}"));
	Ok((version, true))
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

// pulls the first dotted-number token (e.g. "2.1.0" or "21.0.2") out of raw compiler output
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

fn find_kotlin_stdlib_path(
	request: &Request,
	compiler_found: bool,
	tx: &Sender<Event>,
	confirm_rx: &Receiver<bool>,
	skip_rx: &Receiver<()>,
) -> Result<Option<String>, Error> {
	if request.language != Language::Kotlin || !request.kotlin_stdlib {
		log(tx, "Skip kotlin-stdlib.jar search".to_string());
		return Ok(None);
	}
	if !compiler_found {
		log(tx, "Skip kotlin-stdlib.jar search: kotlinc not found".to_string());
		return Ok(None);
	}

	log(tx, "Looking for kotlin-stdlib.jar...".to_string());

	let mut excluded: Vec<std::path::PathBuf> = Vec::new();
	loop {
		drain_skip_signals(skip_rx);
		let _ = tx.send(Event::SkipAvailable(true));
		let outcome = kotlin_stdlib::search(&excluded, skip_rx);
		let _ = tx.send(Event::SkipAvailable(false));

		let found = match outcome {
			kotlin_stdlib::Outcome::Found(path) => path,
			kotlin_stdlib::Outcome::Skipped => {
				log(tx, "Skipped kotlin-stdlib.jar search".to_string());
				return Ok(None);
			}
			kotlin_stdlib::Outcome::NotFound => return handle_not_found(tx, confirm_rx),
		};

		log(tx, format!("Successfully found: {}", found.display()));
		let _ = tx.send(Event::Confirm("Find another one? [y/N]: ".to_string()));
		let find_another = confirm_rx.recv().unwrap_or(false);
		if find_another {
			excluded.push(found);
			continue;
		}

		return install_kotlin_stdlib(&found, tx);
	}
}

fn drain_skip_signals(skip_rx: &Receiver<()>) {
	while skip_rx.try_recv().is_ok() {}
}

fn handle_not_found(tx: &Sender<Event>, confirm_rx: &Receiver<bool>) -> Result<Option<String>, Error> {
	warn(tx, "Warn: Kotlin standard library not found...".to_string());
	let _ = tx.send(Event::Confirm("Continue? [y/N]: ".to_string()));
	let continue_anyway = confirm_rx.recv().unwrap_or(false);
	if continue_anyway {
		return Ok(None);
	}
	log(tx, "Termination...".to_string());
	Err(Error::Terminated)
}

// copies the found jar into ./libs/kotlin-stdlib.jar, the path written into haru.yml
fn install_kotlin_stdlib(found: &Path, tx: &Sender<Event>) -> Result<Option<String>, Error> {
	fs::create_dir_all("libs").map_err(Error::Io)?;
	let dest = Path::new("libs").join("kotlin-stdlib.jar");
	fs::copy(found, &dest).map_err(Error::Io)?;
	log(tx, format!("Copied to {}", dest.display()));
	Ok(Some("./libs/kotlin-stdlib.jar".to_string()))
}

fn write_metadata_yml(request: &Request, tx: &Sender<Event>) -> Result<(), Error> {
	log(tx, "Creating metadata.yml".to_string());

	let source_line = match &request.source {
		Some(source) => format!("source: {source}"),
		None => "# source:".to_string(),
	};

	let contents = format!(
		"id: {}\nversion: 0.1.0\nstate: alpha\nauthor: {}\napp_version: \"{}\"\n{source_line}\n# socials:\n#  - t.me/\n",
		request.sdk_id, request.author, request.app_version,
	);

	fs::write("metadata.yml", contents).map_err(Error::Io)
}

fn write_haru_yml(request: &Request, compiler_version: &str, kotlin_stdlib_path: Option<&str>, tx: &Sender<Event>) -> Result<(), Error> {
	log(tx, "Creating haru.yml".to_string());

	let (kotlinc_line, javac_line, include_kt_line, include_java_line) = match request.language {
		Language::Kotlin => (format!("kotlinc: \">={compiler_version}\""), "# javac:".to_string(), "  - *.kt".to_string(), "  # - *.java".to_string()),
		Language::Java => ("# kotlinc:".to_string(), format!("javac: \">={compiler_version}\""), "  # - *.kt".to_string(), "  - *.java".to_string()),
	};

	let libs_block = match kotlin_stdlib_path {
		Some(path) => format!("libs:\n  - {path}"),
		None => "# libs:\n#   - path/to/jar/kotlin-stdlib.jar".to_string(),
	};

	let contents = format!(
		"class: {}.Main\nmetadata: metadata.yml\nsource: src\n\nbuild: build/latest.dex\n\n{kotlinc_line}\n{javac_line}\n# both are supported, the choice depends on the file format\n\ninclude:\n{include_kt_line}\n{include_java_line}\n\n{libs_block}",
		request.sdk_id,
	);

	fs::write("haru.yml", contents).map_err(Error::Io)
}

fn write_main_file(request: &Request, tx: &Sender<Event>) -> Result<&'static str, Error> {
	let (path, contents): (&'static str, String) = match request.language {
		Language::Kotlin => ("src/Main.kt", format!("fun main() {{\n    println(\"Hello {}\")\n}}", request.author)),
		Language::Java => (
			"src/Main.java",
			format!("public class Main {{\n    public static void main(String[] args) {{\n        System.out.println(\"Hello {}\");\n    }}\n}}", request.author),
		),
	};

	log(tx, format!("Creating {path}"));
	fs::write(path, contents).map_err(Error::Io)?;
	Ok(path)
}
