use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::actions::stubs_parser;
use crate::actions::toolchain::{self, Tool};
use crate::progress::Logger;

const KOTLIN_CACHE_DIR: &str = "build/cache/kotlinc";
const JAVA_CACHE_DIR: &str = "build/cache/javac";
const D8_CACHE_DIR: &str = "build/cache/d8";
const FINAL_DEX: &str = "build/classes.dex";
const KOTLIN_STAGING_DIR: &str = "build/cache/kotlinc-staging";
const AAR_EXTRACT_DIR: &str = "build/cache/aar-extract";

#[derive(Debug)]
pub enum Error {
	UnknownFormat(String),
	CompilerError { component: &'static str, message: String },
	ToolNotFound { component: &'static str },
	AarMissingClasses(String),
	Io(std::io::Error),
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::UnknownFormat(ext) => write!(f, "Unknown file format .{ext}"),
			Self::CompilerError { component, message } => write!(f, "{component} initiated an error:\n{message}"),
			Self::ToolNotFound { component } => write!(f, "{component} not found!"),
			Self::AarMissingClasses(path) => write!(f, "{path} has no classes.jar entry, cannot use it on the classpath"),
			Self::Io(err) => write!(f, "{err}"),
		}
	}
}

impl Error {
	pub fn hint(&self) -> Option<String> {
		let Self::ToolNotFound { component } = self else {
			return None;
		};
		Some(format!("Install {component} and then use:\n    haru config --new {component} \"path/to/{component}\""))
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
	Kotlin,
	Java,
}

struct SourceFile {
	path: PathBuf,
	lang: Lang,
	relative: PathBuf,
}

impl SourceFile {
	fn from_own_source(path: PathBuf, source_path: &str) -> Result<Self, Error> {
		let lang = lang_of_path(&path)?;
		let relative = path.strip_prefix(source_path).unwrap_or(&path).to_path_buf();
		Ok(Self { path, lang, relative })
	}

	fn from_stub_source(path: PathBuf, fqcn: &str) -> Result<Self, Error> {
		let lang = lang_of_path(&path)?;
		let relative = fqcn_to_relative_path(fqcn, lang);
		Ok(Self { path, lang, relative })
	}
}

fn lang_of_path(path: &Path) -> Result<Lang, Error> {
	let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
	match ext.as_str() {
		"kt" => Ok(Lang::Kotlin),
		"java" => Ok(Lang::Java),
		other => Err(Error::UnknownFormat(other.to_string())),
	}
}

// "com.foo.Bar" -> "com/foo/Bar.<ext>"
fn fqcn_to_relative_path(fqcn: &str, lang: Lang) -> PathBuf {
	let ext = match lang {
		Lang::Kotlin => "kt",
		Lang::Java => "java",
	};
	let mut path = PathBuf::new();
	for segment in fqcn.split('.') {
		path.push(segment);
	}
	path.set_extension(ext);
	path
}

pub fn run(haru_yml: &Value, source_path: &str, release: bool, logger: &mut Logger) -> Result<(), Error> {
	let sources = collect_sources(haru_yml, source_path)?;
	logger.extend_total(sources.len() as u32);

	let class_files = compile_sources(haru_yml, &sources, source_path, logger)?;
	logger.extend_total(class_files.len() as u32);

	let static_libs = resolve_static_libs_for_dex(haru_yml, logger)?;
	let dex_files = dex_classes(&class_files, &static_libs, logger)?;

	merge_dex(&dex_files, &static_libs, release, logger)?;
	logger.step();

	Ok(())
}

fn collect_sources(haru_yml: &Value, source_path: &str) -> Result<Vec<SourceFile>, Error> {
	let masks = read_include_masks(haru_yml);
	let all_files = walk_dir(Path::new(source_path)).map_err(Error::Io)?;

	let mut sources = Vec::new();
	for path in all_files {
		let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
		if !masks.iter().any(|mask| mask_matches(mask, &file_name)) {
			continue;
		}
		sources.push(SourceFile::from_own_source(path, source_path)?);
	}
	Ok(sources)
}

fn read_include_masks(haru_yml: &Value) -> Vec<String> {
	let Some(masks) = haru_yml.get("include").and_then(Value::as_array) else {
		return Vec::new();
	};
	masks.iter().filter_map(Value::as_str).map(str::to_string).collect()
}

// simple glob: only "*.ext" masks are used in haru.yml, matched by suffix
fn mask_matches(mask: &str, file_name: &str) -> bool {
	match mask.strip_prefix('*') {
		Some(suffix) => file_name.ends_with(suffix),
		None => mask == file_name,
	}
}

fn walk_dir(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
	let mut files = Vec::new();
	for entry in fs::read_dir(dir)? {
		let entry = entry?;
		let path = entry.path();
		if path.is_dir() {
			files.extend(walk_dir(&path)?);
		} else {
			files.push(path);
		}
	}
	Ok(files)
}

fn compile_sources(haru_yml: &Value, sources: &[SourceFile], source_path: &str, logger: &mut Logger) -> Result<Vec<PathBuf>, Error> {
	let classpath = build_classpath(haru_yml, logger)?;
	let stub_sources = stub_sources_needed(haru_yml, source_path, &classpath, logger)?;

	let all_sources: Vec<&SourceFile> = sources.iter().chain(stub_sources.iter()).collect();
	let (kotlin_sources, java_sources): (Vec<&SourceFile>, Vec<&SourceFile>) = all_sources.into_iter().partition(|s| s.lang == Lang::Kotlin);

	let mut class_files = Vec::new();

	if !kotlin_sources.is_empty() {
		let joint_sources: Vec<&SourceFile> = kotlin_sources.iter().copied().chain(java_sources.iter().copied()).collect();
		class_files.extend(compile_kotlin_sources(&joint_sources, &classpath, logger)?);
	}

	if !java_sources.is_empty() {
		prune_java_cache(&java_sources).map_err(Error::Io)?;
		class_files.extend(compile_java_sources(&java_sources, &classpath, logger)?);
	}

	Ok(class_files)
}

fn stub_sources_needed(haru_yml: &Value, source_path: &str, classpath: &[String], logger: &mut Logger) -> Result<Vec<SourceFile>, Error> {
	let stub_source_dirs = read_stub_source_dirs(haru_yml);
	if stub_source_dirs.is_empty() {
		logger.debug("no stub source directories configured, skipping stub resolution");
		return Ok(Vec::new());
	}

	for dir in &stub_source_dirs {
		logger.debug(&format!("stub source root: {}", dir.display()));
	}

	let stub_roots: Vec<&Path> = stub_source_dirs.iter().map(PathBuf::as_path).collect();
	let resolution = stubs_parser::resolve(Path::new(source_path), &stub_roots, classpath).map_err(Error::Io)?;

	let d = &resolution.diagnostics;
	logger.debug(&format!("scanned {} source files across main root + stub roots", d.scanned_files));
	logger.debug(&format!("{} fqcns declared under main source root", d.main_fqcns));
	logger.debug(&format!("transitive closure from main root: {} fqcns", d.closure_size));
	logger.debug(&format!("skipped {} main-root fqcns, {} fqcns already in a jar", d.skipped_main, d.skipped_in_jar));
	logger.debug(&format!("{} stub files required for compilation", resolution.required_files.len()));

	let mut resolved = Vec::new();
	for required in resolution.required_files {
		logger.debug(&format!("stub resolved: {} -> {}", required.fqcn, required.path.display()));
		logger.log(&format!("Including stub source {}", required.path.display()));
		resolved.push(SourceFile::from_stub_source(required.path, &required.fqcn)?);
	}
	Ok(resolved)
}

fn build_classpath(haru_yml: &Value, logger: &mut Logger) -> Result<Vec<String>, Error> {
	let mut entries = Vec::new();
	for path in read_static_libs(haru_yml).into_iter().chain(read_stubs(haru_yml)) {
		let entry_path = Path::new(&path);
		if entry_path.is_dir() {
			logger.debug(&format!("{path} is a directory, not added to classpath (handled as stub source root)"));
			continue;
		}
		let Some(resolved) = resolve_lib_entry(&path, logger)? else {
			logger.log(&format!("Skipping {path} on the compiler classpath: not a .jar/.aar (kotlinc/javac can't read .apk/.dex bytecode)"));
			continue;
		};
		entries.push(resolved);
	}
	logger.debug(&format!("classpath: {}", entries.join(", ")));
	Ok(entries)
}

fn resolve_lib_entry(path: &str, logger: &mut Logger) -> Result<Option<String>, Error> {
	let entry_path = Path::new(path);
	match entry_path.extension().and_then(|e| e.to_str()) {
		Some("jar") => Ok(Some(path.to_string())),
		Some("aar") => {
			let extracted = extract_aar_classes(entry_path)?;
			logger.debug(&format!("{path} extracted to {}", extracted.display()));
			Ok(Some(extracted.to_string_lossy().into_owned()))
		}
		_ => Ok(None),
	}
}

fn extract_aar_classes(aar_path: &Path) -> Result<PathBuf, Error> {
	let name = aar_path.file_stem().and_then(|s| s.to_str()).unwrap_or("aar");
	let out_dir = Path::new(AAR_EXTRACT_DIR).join(name);
	let out_jar = out_dir.join("classes.jar");

	fs::create_dir_all(&out_dir).map_err(Error::Io)?;

	let file = fs::File::open(aar_path).map_err(Error::Io)?;
	let mut archive = zip::ZipArchive::new(file).map_err(|err| Error::Io(std::io::Error::other(err)))?;
	let mut entry = archive
		.by_name("classes.jar")
		.map_err(|_| Error::AarMissingClasses(aar_path.display().to_string()))?;

	let mut out_file = fs::File::create(&out_jar).map_err(Error::Io)?;
	std::io::copy(&mut entry, &mut out_file).map_err(Error::Io)?;

	Ok(out_jar)
}

fn resolve_static_libs_for_dex(haru_yml: &Value, logger: &mut Logger) -> Result<Vec<String>, Error> {
	let mut entries = Vec::new();
	for path in read_static_libs(haru_yml) {
		match resolve_lib_entry(&path, logger)? {
			Some(resolved) => entries.push(resolved),
			None => logger.log(&format!("Skipping {path} as a d8 --lib: not a .jar/.aar")),
		}
	}
	Ok(entries)
}

fn read_stubs(haru_yml: &Value) -> Vec<String> {
	let Some(stubs) = haru_yml.get("stubs").and_then(Value::as_array) else {
		return Vec::new();
	};
	stubs.iter().filter_map(Value::as_str).map(str::to_string).collect()
}

fn read_stub_source_dirs(haru_yml: &Value) -> Vec<PathBuf> {
	read_stubs(haru_yml).into_iter().map(PathBuf::from).filter(|path| path.is_dir()).collect()
}

fn prune_java_cache(sources: &[&SourceFile]) -> std::io::Result<()> {
	let cache_dir = Path::new(JAVA_CACHE_DIR);
	if !cache_dir.exists() {
		return Ok(());
	}

	let expected_stems: std::collections::HashSet<PathBuf> = sources.iter().map(|source| source.relative.with_extension("")).collect();

	for class_path in walk_dir(cache_dir)? {
		if class_path.extension().and_then(|e| e.to_str()) != Some("class") {
			continue;
		}
		let relative = class_path.strip_prefix(cache_dir).unwrap_or(&class_path);
		let stem = strip_nested_class_suffix(&relative.with_extension(""));
		if !expected_stems.contains(&stem) {
			let _ = fs::remove_file(&class_path);
		}
	}

	remove_empty_dirs(cache_dir)?;
	Ok(())
}

// "com/foo/Bar$Inner" -> "com/foo/Bar"
fn strip_nested_class_suffix(path: &Path) -> PathBuf {
	let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
		return path.to_path_buf();
	};
	let Some((base, _)) = file_name.split_once('$') else {
		return path.to_path_buf();
	};
	path.with_file_name(base)
}

fn class_belongs_to(class_path: &Path, kotlin_relatives: &std::collections::HashSet<&PathBuf>) -> bool {
	let relative = class_path.strip_prefix(KOTLIN_CACHE_DIR).unwrap_or(class_path);
	let stem = strip_nested_class_suffix(&relative.with_extension(""));
	kotlin_relatives.contains(&stem)
}

fn remove_empty_dirs(dir: &Path) -> std::io::Result<()> {
	for entry in fs::read_dir(dir)? {
		let entry = entry?;
		let path = entry.path();
		if !path.is_dir() {
			continue;
		}
		remove_empty_dirs(&path)?;
		if fs::read_dir(&path)?.next().is_none() {
			fs::remove_dir(&path)?;
		}
	}
	Ok(())
}

fn compile_kotlin_sources(sources: &[&SourceFile], classpath: &[String], logger: &mut Logger) -> Result<Vec<PathBuf>, Error> {
	let staging_dir = Path::new(KOTLIN_STAGING_DIR);
	let kotlin_sources: Vec<&&SourceFile> = sources.iter().filter(|s| s.lang == Lang::Kotlin).collect();

	for source in sources {
		logger.debug(&format!("staging {} -> {}", source.path.display(), staging_dir.join(&source.relative).display()));
	}

	stage_sources(sources, staging_dir).map_err(Error::Io)?;

	for source in &kotlin_sources {
		let class_path = Path::new(KOTLIN_CACHE_DIR).join(source.relative.with_extension("class"));
		logger.log(&format!("Compiling {} to {}", source.path.display(), class_path.display()));
	}

	let result = run_kotlinc_batch(staging_dir, classpath);

	let _ = fs::remove_dir_all(staging_dir);

	result?;

	for source in &kotlin_sources {
		logger.log(&format!("Compiled {}", source.path.display()));
		logger.step();
	}

	let kotlin_relatives: std::collections::HashSet<&PathBuf> = kotlin_sources.iter().map(|s| &s.relative).collect();
	let produced = classes_produced(Path::new(KOTLIN_CACHE_DIR))?;
	Ok(produced.into_iter().filter(|p| class_belongs_to(p, &kotlin_relatives)).collect())
}

fn stage_sources(sources: &[&SourceFile], staging_dir: &Path) -> std::io::Result<()> {
	if staging_dir.exists() {
		fs::remove_dir_all(staging_dir)?;
	}
	fs::create_dir_all(staging_dir)?;

	for source in sources {
		let dest = staging_dir.join(&source.relative);
		if let Some(parent) = dest.parent() {
			fs::create_dir_all(parent)?;
		}
		fs::copy(&source.path, &dest)?;
	}
	Ok(())
}

fn run_kotlinc_batch(staging_dir: &Path, classpath: &[String]) -> Result<(), Error> {
	let cache_dir = Path::new(KOTLIN_CACHE_DIR);
	if cache_dir.exists() {
		fs::remove_dir_all(cache_dir).map_err(Error::Io)?;
	}
	fs::create_dir_all(cache_dir).map_err(Error::Io)?;
	let binary = locate_tool(Tool::Kotlinc, "kotlinc")?;

	let mut command = Command::new(&binary);
	command.arg(staging_dir).arg("-d").arg(cache_dir);
	append_classpath(&mut command, classpath);
	run_compiler(&mut command, "kotlinc")
}

fn compile_java_sources(sources: &[&SourceFile], classpath: &[String], logger: &mut Logger) -> Result<Vec<PathBuf>, Error> {
	let cache_dir = Path::new(JAVA_CACHE_DIR);
	fs::create_dir_all(cache_dir).map_err(Error::Io)?;

	for source in sources {
		let class_path = cache_dir.join(source.relative.with_extension("class"));
		logger.log(&format!("Compiling {} to {}", source.path.display(), class_path.display()));
	}

	let paths: Vec<&Path> = sources.iter().map(|s| s.path.as_path()).collect();
	run_javac_batch(&paths, cache_dir, classpath)?;

	for source in sources {
		logger.log(&format!("Compiled {}", source.path.display()));
		logger.step();
	}

	classes_produced(cache_dir)
}

fn run_javac_batch(sources: &[&Path], target_dir: &Path, classpath: &[String]) -> Result<(), Error> {
	let binary = locate_tool(Tool::Javac, "javac")?;
	let mut command = Command::new(&binary);
	command.args(sources).arg("-d").arg(target_dir);
	append_classpath(&mut command, classpath);
	run_compiler(&mut command, "javac")
}

fn append_classpath(command: &mut Command, classpath: &[String]) {
	if classpath.is_empty() {
		return;
	}
	let separator = if cfg!(windows) { ";" } else { ":" };
	command.arg("-cp").arg(classpath.join(separator));
}

fn run_compiler(command: &mut Command, component: &'static str) -> Result<(), Error> {
	let output = command.output().map_err(Error::Io)?;
	if !output.status.success() {
		let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
		return Err(Error::CompilerError { component, message });
	}
	Ok(())
}

fn classes_produced(dir: &Path) -> Result<Vec<PathBuf>, Error> {
	let mut classes = Vec::new();
	for path in walk_dir(dir).map_err(Error::Io)? {
		if path.extension().and_then(|e| e.to_str()) == Some("class") {
			classes.push(path);
		}
	}
	Ok(classes)
}

fn dex_classes(class_files: &[PathBuf], static_libs: &[String], logger: &mut Logger) -> Result<Vec<PathBuf>, Error> {
	fs::create_dir_all(D8_CACHE_DIR).map_err(Error::Io)?;
	prune_d8_cache(class_files).map_err(Error::Io)?;

	let mut dex_files = Vec::new();
	for class_file in class_files {
		let dex_path = dex_one(class_file, static_libs, logger)?;
		dex_files.push(dex_path);
	}
	Ok(dex_files)
}

fn prune_d8_cache(class_files: &[PathBuf]) -> std::io::Result<()> {
	let cache_dir = Path::new(D8_CACHE_DIR);
	if !cache_dir.exists() {
		return Ok(());
	}

	let expected_keys: std::collections::HashSet<String> =
		class_files.iter().map(|class_file| class_file.with_extension("").to_string_lossy().replace(['/', '\\'], "_")).collect();

	for entry in fs::read_dir(cache_dir)? {
		let entry = entry?;
		let path = entry.path();
		if !path.is_dir() {
			continue;
		}
		let Some(key) = path.file_name().and_then(|n| n.to_str()) else {
			continue;
		};
		if !expected_keys.contains(key) {
			fs::remove_dir_all(&path)?;
		}
	}
	Ok(())
}

fn read_static_libs(haru_yml: &Value) -> Vec<String> {
	let Some(libs) = haru_yml.get("static-libs").and_then(Value::as_array) else {
		return Vec::new();
	};
	libs.iter().filter_map(Value::as_str).map(str::to_string).collect()
}

fn dex_one(class_file: &Path, static_libs: &[String], logger: &mut Logger) -> Result<PathBuf, Error> {
	let binary = locate_tool(Tool::D8, "d8")?;
	let key = class_file.with_extension("").to_string_lossy().replace(['/', '\\'], "_");
	let dex_dir = Path::new(D8_CACHE_DIR).join(&key);
	fs::create_dir_all(&dex_dir).map_err(Error::Io)?;

	logger.log(&format!("Converting {} to .dex", class_file.display()));

	let mut command = Command::new(&binary);
	command.arg(class_file).arg("--output").arg(&dex_dir);
	for lib in static_libs {
		command.arg("--lib").arg(lib);
	}

	run_compiler(&mut command, "d8")?;
	logger.log(&format!("Dexed {}", class_file.display()));
	logger.step();
	Ok(dex_dir.join("classes.dex"))
}

fn merge_dex(dex_files: &[PathBuf], static_libs: &[String], release: bool, logger: &mut Logger) -> Result<(), Error> {
	let binary = locate_tool(Tool::D8, "d8")?;

	let out_dir = Path::new(FINAL_DEX).parent().unwrap_or(Path::new("build"));
	fs::create_dir_all(out_dir).map_err(Error::Io)?;

	logger.log(&format!("Merging dexes into {FINAL_DEX}"));

	let mut command = Command::new(&binary);
	for dex in dex_files {
		command.arg(dex);
	}
	command.arg("--output").arg(out_dir);
	for lib in static_libs {
		command.arg("--lib").arg(lib);
	}
	if release {
		command.arg("--release");
	}

	run_compiler(&mut command, "d8")?;

	let produced = out_dir.join("classes.dex");
	if produced != Path::new(FINAL_DEX) {
		fs::rename(&produced, FINAL_DEX).map_err(Error::Io)?;
	}
	logger.log(&format!("Merged into {FINAL_DEX}"));
	Ok(())
}

fn locate_tool(tool: Tool, component: &'static str) -> Result<PathBuf, Error> {
	toolchain::locate(tool).ok_or(Error::ToolNotFound { component })
}
