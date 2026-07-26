use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::actions::res_gen;
use crate::actions::toolchain::{self, Tool};
use crate::progress::Logger;

const KOTLIN_CACHE_DIR: &str = "build/cache/kotlinc";
const JAVA_CACHE_DIR: &str = "build/cache/javac";
const JAVA_STUB_CACHE_DIR: &str = "build/cache/javac-stubs";
const D8_CACHE_DIR: &str = "build/cache/d8";
const FINAL_DEX: &str = "build/classes.dex";
const KOTLIN_STAGING_DIR: &str = "build/cache/kotlinc-staging";
const AAR_EXTRACT_DIR: &str = "build/cache/aar-extract";
const R_CACHE_DIR: &str = "build/cache/r";

#[derive(Debug)]
pub enum Error {
	UnknownFormat(String),
	CompilerError { component: &'static str, message: String },
	ToolNotFound { component: &'static str },
	AarMissingClasses(String),
	NoClassesProduced,
	Io(std::io::Error),
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::UnknownFormat(ext) => write!(f, "Unknown file format .{ext}"),
			Self::CompilerError { component, message } => write!(f, "{component} initiated an error:\n{message}"),
			Self::ToolNotFound { component } => write!(f, "{component} not found!"),
			Self::AarMissingClasses(path) => write!(f, "{path} has no classes.jar entry, cannot use it on the classpath"),
			Self::NoClassesProduced => write!(f, "the compilers reported no class files for this project's sources"),
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

}

fn lang_of_path(path: &Path) -> Result<Lang, Error> {
	let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
	match ext.as_str() {
		"kt" => Ok(Lang::Kotlin),
		"java" => Ok(Lang::Java),
		other => Err(Error::UnknownFormat(other.to_string())),
	}
}

pub fn run(haru_yml: &Value, source_path: &str, release: bool, jvm_args: &[String], maven_libs: &[String], logger: &mut Logger) -> Result<(), Error> {
	let sources = collect_sources(haru_yml, source_path)?;
	logger.extend_total(sources.len() as u32);

	let class_files = compile_sources(haru_yml, &sources, jvm_args, maven_libs, logger)?;
	logger.extend_total(class_files.len() as u32);

	let static_libs = resolve_static_libs_for_dex(haru_yml, maven_libs, logger)?;
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

fn compile_sources(haru_yml: &Value, sources: &[SourceFile], jvm_args: &[String], maven_libs: &[String], logger: &mut Logger) -> Result<Vec<PathBuf>, Error> {
	let classpath = build_classpath(haru_yml, maven_libs, logger)?;
	let stub_roots = stub_source_roots(haru_yml, logger);

	let (kotlin_sources, java_sources): (Vec<&SourceFile>, Vec<&SourceFile>) = sources.iter().partition(|s| s.lang == Lang::Kotlin);

	let mut class_files = Vec::new();

	if !kotlin_sources.is_empty() {
		let joint_sources: Vec<&SourceFile> = kotlin_sources.iter().copied().chain(java_sources.iter().copied()).collect();
		class_files.extend(compile_kotlin_sources(&joint_sources, &classpath, &stub_roots, jvm_args, logger)?);
	}

	if !java_sources.is_empty() {
		let java_classpath = classpath_with_stub_aux_classes(&classpath, &stub_roots, jvm_args, logger)?;
		class_files.extend(compile_java_sources(&java_sources, &java_classpath, &stub_roots, jvm_args, logger)?);
	}

	// every source we were given had to contribute at least one class; ending up with none means
	// the compilers silently wrote nothing, and packaging would then ship the previous build's dex
	if class_files.is_empty() {
		return Err(Error::NoClassesProduced);
	}

	Ok(class_files)
}

// javac finds a class on -sourcepath only in the file named after it, so a secondary top-level
// type such as org.telegram.ui.Stories.HwFrameLayout (declared inside HwLayouts.java) stays
// invisible no matter how the sourcepath is set up. kotlinc indexes whole source roots and has no
// such limit. The few stub files that declare one are compiled up front into their own directory,
// which then joins the classpath so javac can resolve those types like any other library class.
fn classpath_with_stub_aux_classes(classpath: &[String], stub_roots: &[PathBuf], jvm_args: &[String], logger: &mut Logger) -> Result<Vec<String>, Error> {
	let aux_sources = stub_sources_with_secondary_types(stub_roots).map_err(Error::Io)?;
	if aux_sources.is_empty() {
		return Ok(classpath.to_vec());
	}

	let cache_dir = Path::new(JAVA_STUB_CACHE_DIR);
	if cache_dir.exists() {
		fs::remove_dir_all(cache_dir).map_err(Error::Io)?;
	}
	fs::create_dir_all(cache_dir).map_err(Error::Io)?;

	for source in &aux_sources {
		logger.debug(&format!("stub file with secondary top-level types: {}", source.display()));
	}
	logger.log(&format!("Compiling {} stub files that declare secondary top-level types", aux_sources.len()));

	let paths: Vec<&Path> = aux_sources.iter().map(PathBuf::as_path).collect();
	run_javac_batch(&paths, cache_dir, classpath, stub_roots, jvm_args, logger)?;

	let mut extended = classpath.to_vec();
	extended.push(JAVA_STUB_CACHE_DIR.to_string());
	Ok(extended)
}

fn stub_sources_with_secondary_types(stub_roots: &[PathBuf]) -> std::io::Result<Vec<PathBuf>> {
	let mut sources = Vec::new();
	for root in stub_roots {
		for path in walk_dir(root)? {
			if path.extension().and_then(|e| e.to_str()) != Some("java") {
				continue;
			}
			let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
				continue;
			};
			let contents = fs::read_to_string(&path)?;
			if top_level_type_names(&contents).iter().any(|name| name != stem) {
				sources.push(path);
			}
		}
	}
	Ok(sources)
}

// names of the types declared at brace depth 0, which is where a java file's top-level types sit
fn top_level_type_names(contents: &str) -> Vec<String> {
	const KEYWORDS: [&str; 4] = ["class", "interface", "enum", "record"];

	let mut names = Vec::new();
	let mut depth: i32 = 0;
	for word in strip_comments_and_literals(contents).split_whitespace().collect::<Vec<_>>().windows(2) {
		let (current, next) = (word[0], word[1]);
		if depth == 0 && KEYWORDS.contains(&current.trim_start_matches('@')) {
			let name: String = next.chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect();
			if !name.is_empty() {
				names.push(name);
			}
		}
		depth += current.matches('{').count() as i32 - current.matches('}').count() as i32;
	}
	names
}

// blanks out comments and string/char literals so braces and keywords inside them are not counted
fn strip_comments_and_literals(contents: &str) -> String {
	#[derive(PartialEq)]
	enum Mode {
		Code,
		LineComment,
		BlockComment,
		Text(char),
	}

	let mut out = String::with_capacity(contents.len());
	let mut mode = Mode::Code;
	let mut chars = contents.chars().peekable();
	while let Some(ch) = chars.next() {
		match mode {
			Mode::Code => match ch {
				'/' if chars.peek() == Some(&'/') => mode = Mode::LineComment,
				'/' if chars.peek() == Some(&'*') => mode = Mode::BlockComment,
				'"' | '\'' => mode = Mode::Text(ch),
				_ => out.push(ch),
			},
			Mode::LineComment => {
				if ch == '\n' {
					mode = Mode::Code;
					out.push(ch);
				}
			}
			Mode::BlockComment => {
				if ch == '*' && chars.peek() == Some(&'/') {
					chars.next();
					mode = Mode::Code;
				}
			}
			Mode::Text(quote) => match ch {
				'\\' => {
					chars.next();
				}
				_ if ch == quote => mode = Mode::Code,
				_ => {}
			},
		}
		// a separator keeps the blanked-out region from gluing the words around it together
		if mode != Mode::Code && !out.ends_with(' ') {
			out.push(' ');
		}
	}
	out
}

// Stubs describe classes the host app already ships, so they are never compiled into the output:
// they are handed to the compilers as source roots (kotlinc) and -sourcepath (javac), which lets
// each compiler pull in exactly the declarations it needs to type-check our sources.
// The generated R/BuildConfig cache is a stub root too — those classes come from the host as well.
fn stub_source_roots(haru_yml: &Value, logger: &mut Logger) -> Vec<PathBuf> {
	let mut roots = read_stub_source_dirs(haru_yml);
	let r_cache_dir = PathBuf::from(R_CACHE_DIR);
	if r_cache_dir.is_dir() {
		roots.push(r_cache_dir);
	}
	for root in &roots {
		logger.debug(&format!("stub source root: {}", root.display()));
	}
	roots
}

fn build_classpath(haru_yml: &Value, maven_libs: &[String], logger: &mut Logger) -> Result<Vec<String>, Error> {
	let mut entries = Vec::new();
	for path in read_static_libs(haru_yml).into_iter().chain(read_stubs(haru_yml)) {
		let entry_path = Path::new(&path);
		if entry_path.is_dir() {
			logger.debug(&format!("{path} is a directory, not added to classpath (handled as stub source root)"));
			continue;
		}
		let Some(resolved) = resolve_lib_entry(&path, logger)? else {
			logger.log(&format!("Skipping {path} on the compiler classpath"));
			continue;
		};
		entries.push(resolved);
	}
	entries.extend(maven_libs.iter().cloned());
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

fn resolve_static_libs_for_dex(haru_yml: &Value, maven_libs: &[String], logger: &mut Logger) -> Result<Vec<String>, Error> {
	let mut entries = Vec::new();
	for path in read_static_libs(haru_yml) {
		match resolve_lib_entry(&path, logger)? {
			Some(resolved) => entries.push(resolved),
			None => logger.log(&format!("Skipping {path} as a d8 --lib")),
		}
	}
	entries.extend(maven_libs.iter().cloned());
	Ok(entries)
}

fn read_stubs(haru_yml: &Value) -> Vec<String> {
	let Some(stubs) = haru_yml.get("stubs").and_then(Value::as_array) else {
		return Vec::new();
	};
	stubs.iter().filter_map(Value::as_str).map(|raw| res_gen::split_stub_entry(raw).java_path.to_string()).collect()
}

fn read_stub_source_dirs(haru_yml: &Value) -> Vec<PathBuf> {
	read_stubs(haru_yml).into_iter().map(PathBuf::from).filter(|path| path.is_dir()).collect()
}

fn compile_kotlin_sources(sources: &[&SourceFile], classpath: &[String], stub_roots: &[PathBuf], jvm_args: &[String], logger: &mut Logger) -> Result<Vec<PathBuf>, Error> {
	let staging_dir = Path::new(KOTLIN_STAGING_DIR);
	let kotlin_sources: Vec<&&SourceFile> = sources.iter().filter(|s| s.lang == Lang::Kotlin).collect();

	for source in sources {
		logger.debug(&format!("staging {} -> {}", source.path.display(), staging_dir.join(&source.relative).display()));
	}

	stage_sources(sources, staging_dir).map_err(Error::Io)?;
	let staging_root = absolute(staging_dir);

	for source in &kotlin_sources {
		logger.log(&format!("Compiling {}", source.path.display()));
	}

	let result = run_kotlinc_batch(staging_dir, stub_roots, classpath, jvm_args);

	let _ = fs::remove_dir_all(staging_dir);

	let report = result?;
	log_compiler_diagnostics("kotlinc", &report, logger);

	for source in &kotlin_sources {
		logger.log(&format!("Compiled {}", source.path.display()));
		logger.step();
	}

	let produced = staged_classes_from_report(&report, &staging_root, &absolute(Path::new(KOTLIN_CACHE_DIR)));
	for class_file in &produced {
		logger.debug(&format!("kotlinc produced {}", class_file.display()));
	}
	Ok(produced)
}

// kotlinc gets the stub roots as extra source roots, so its output directory also holds classes
// compiled from stub sources. -Xreport-output-files makes it report every class it wrote together
// with the sources it came from; a class is ours exactly when one of those sources is a staged one.
// Records look like this, and the source list runs until the next marker:
//     output: output:
//     <path of the written .class>
//     Sources:
//     <path of a source file>
// kotlinc resolves the source paths but echoes the output path as it was passed in -d, so both
// sides are made absolute before they are compared.
fn staged_classes_from_report(report: &str, staging_root: &Path, cache_root: &Path) -> Vec<PathBuf> {
	const MARKER: &str = "output: output:";
	const SOURCES: &str = "Sources:";

	let lines: Vec<&str> = report.lines().map(str::trim).collect();
	let mut classes = Vec::new();
	let mut index = 0;
	while index < lines.len() {
		if lines[index] != MARKER || lines.get(index + 2) != Some(&SOURCES) {
			index += 1;
			continue;
		}
		let class_file = absolute(Path::new(lines[index + 1]));
		let mut cursor = index + 3;
		let mut staged = false;
		while cursor < lines.len() && lines[cursor] != MARKER {
			staged |= absolute(Path::new(lines[cursor])).starts_with(staging_root);
			cursor += 1;
		}
		// kotlinc also reports non-class outputs such as META-INF/*.kotlin_module, which d8 rejects
		let is_class = class_file.extension().and_then(|e| e.to_str()) == Some("class");
		if staged && is_class {
			// rebased onto the relative cache dir so the d8 cache keys stay short and portable
			let relative = class_file.strip_prefix(cache_root).unwrap_or(&class_file);
			classes.push(Path::new(KOTLIN_CACHE_DIR).join(relative));
		}
		index = cursor;
	}
	classes
}

// a successful compiler run still has warnings worth seeing under -v 2, and they are otherwise
// swallowed together with the exit status. Only the diagnostic lines are kept: kotlinc shares this
// stream with its -Xreport-output-files records, which run to thousands of lines.
fn log_compiler_diagnostics(component: &str, diagnostics: &str, logger: &mut Logger) {
	for line in diagnostics.lines().map(str::trim_end) {
		if ["warning:", "error:", "note:", "Note:"].iter().any(|marker| line.contains(marker)) {
			logger.debug(&format!("{component}: {}", line.trim_start()));
		}
	}
}

// lexical only: it never touches the filesystem, so it also works for paths already cleaned up,
// and it leaves symlinks unresolved the same way java's getAbsolutePath does
fn absolute(path: &Path) -> PathBuf {
	std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
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

fn run_kotlinc_batch(staging_dir: &Path, stub_roots: &[PathBuf], classpath: &[String], jvm_args: &[String]) -> Result<String, Error> {
	let cache_dir = Path::new(KOTLIN_CACHE_DIR);
	if cache_dir.exists() {
		fs::remove_dir_all(cache_dir).map_err(Error::Io)?;
	}
	fs::create_dir_all(cache_dir).map_err(Error::Io)?;
	let binary = locate_tool(Tool::Kotlinc, "kotlinc")?;

	let mut command = Command::new(&binary);
	command.arg(staging_dir);
	// extra source roots are resolution material only; whatever they produce is dropped by
	// staged_classes_from_report, which keeps the output limited to the project's own sources
	command.args(stub_roots);
	command.arg("-d").arg(cache_dir);
	// makes kotlinc report which sources every written class came from, so the stub classes it
	// emits alongside ours can be told apart afterwards
	command.arg("-Xreport-output-files");
	append_classpath(&mut command, classpath);
	append_jvm_args(&mut command, jvm_args);
	run_compiler(&mut command, "kotlinc")
}

fn compile_java_sources(sources: &[&SourceFile], classpath: &[String], stub_roots: &[PathBuf], jvm_args: &[String], logger: &mut Logger) -> Result<Vec<PathBuf>, Error> {
	// wiped rather than pruned, like the kotlinc one: -implicit:none keeps javac from writing
	// anything but these sources' own classes, so a clean directory is exactly the build output
	// and a class left over from a source that has since been deleted cannot reach the dex
	let cache_dir = Path::new(JAVA_CACHE_DIR);
	if cache_dir.exists() {
		fs::remove_dir_all(cache_dir).map_err(Error::Io)?;
	}
	fs::create_dir_all(cache_dir).map_err(Error::Io)?;

	for source in sources {
		let class_path = cache_dir.join(source.relative.with_extension("class"));
		logger.log(&format!("Compiling {} to {}", source.path.display(), class_path.display()));
	}

	let paths: Vec<&Path> = sources.iter().map(|s| s.path.as_path()).collect();
	run_javac_batch(&paths, cache_dir, classpath, stub_roots, jvm_args, logger)?;

	for source in sources {
		logger.log(&format!("Compiled {}", source.path.display()));
		logger.step();
	}

	classes_produced(cache_dir)
}

fn run_javac_batch(sources: &[&Path], target_dir: &Path, classpath: &[String], stub_roots: &[PathBuf], jvm_args: &[String], logger: &mut Logger) -> Result<(), Error> {
	let binary = locate_tool(Tool::Javac, "javac")?;
	let mut command = Command::new(&binary);
	command.args(sources).arg("-d").arg(target_dir);
	append_classpath(&mut command, classpath);
	append_sourcepath(&mut command, stub_roots);
	append_jvm_args(&mut command, jvm_args);
	let diagnostics = run_compiler(&mut command, "javac")?;
	log_compiler_diagnostics("javac", &diagnostics, logger);
	Ok(())
}

fn append_jvm_args(command: &mut Command, jvm_args: &[String]) {
	for arg in jvm_args {
		command.arg(format!("-J{arg}"));
	}
}

fn append_classpath(command: &mut Command, classpath: &[String]) {
	if classpath.is_empty() {
		return;
	}
	command.arg("-cp").arg(classpath.join(path_separator()));
}

fn append_sourcepath(command: &mut Command, stub_roots: &[PathBuf]) {
	if stub_roots.is_empty() {
		return;
	}
	let joined = stub_roots.iter().map(|root| root.to_string_lossy().into_owned()).collect::<Vec<_>>().join(path_separator());
	command.arg("-sourcepath").arg(joined);
	// implicit:none keeps stub classes out of the output, prefer:source makes a stub source win
	// over an outdated copy of the same class pulled in transitively from the classpath
	command.arg("-implicit:none").arg("-Xprefer:source");
}

fn path_separator() -> &'static str {
	if cfg!(windows) { ";" } else { ":" }
}

// returns the compiler's diagnostic stream, which is also where kotlinc writes its output report
fn run_compiler(command: &mut Command, component: &'static str) -> Result<String, Error> {
	let output = command.output().map_err(Error::Io)?;
	let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
	if !output.status.success() {
		return Err(Error::CompilerError { component, message: stderr.trim().to_string() });
	}
	Ok(stderr)
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn finds_only_types_declared_at_top_level() {
		let source = "package p;\n\nclass Outer {\n\tclass Nested {}\n\tvoid f() { new Runnable() {}; }\n}\n\nclass Secondary {}\n";
		assert_eq!(top_level_type_names(source), vec!["Outer".to_string(), "Secondary".to_string()]);
	}

	#[test]
	fn braces_inside_comments_and_literals_do_not_shift_the_depth() {
		let source = "class A {\n\tString s = \"{{{\";\n\tchar c = '}';\n\t// }}}\n\t/* } */\n}\n\nclass B {}\n";
		assert_eq!(top_level_type_names(source), vec!["A".to_string(), "B".to_string()]);
	}

	#[test]
	fn reads_annotation_and_enum_declarations() {
		let source = "package p;\n@interface Marker {}\nenum Color { RED }\nrecord Point(int x) {}\n";
		assert_eq!(top_level_type_names(source), vec!["Marker".to_string(), "Color".to_string(), "Point".to_string()]);
	}

	#[test]
	fn keeps_classes_whose_kotlinc_record_lists_a_staged_source() {
		let report = concat!(
			"output: output:\n/w/build/cache/kotlinc/p/MineKt.class\nSources:\n/w/build/cache/kotlinc-staging/Mine.kt\n",
			"output: output:\n/w/build/cache/kotlinc/q/Stub.class\nSources:\n/w/stubs/q/Stub.kt\n",
			"output: output:\n/w/build/cache/kotlinc/META-INF/main.kotlin_module\nSources:\n/w/build/cache/kotlinc-staging/Mine.kt\n",
		);
		let classes = staged_classes_from_report(report, Path::new("/w/build/cache/kotlinc-staging"), Path::new("/w/build/cache/kotlinc"));
		assert_eq!(classes, vec![Path::new(KOTLIN_CACHE_DIR).join("p/MineKt.class")]);
	}

	#[test]
	fn keeps_a_multi_source_class_when_any_of_its_sources_is_staged() {
		let report = "output: output:\n/w/build/cache/kotlinc/p/FacadeKt.class\nSources:\n/w/stubs/p/Other.kt\n/w/build/cache/kotlinc-staging/Mine.kt\n";
		let classes = staged_classes_from_report(report, Path::new("/w/build/cache/kotlinc-staging"), Path::new("/w/build/cache/kotlinc"));
		assert_eq!(classes, vec![Path::new(KOTLIN_CACHE_DIR).join("p/FacadeKt.class")]);
	}

	#[test]
	fn reports_no_classes_when_kotlinc_wrote_nothing() {
		assert!(staged_classes_from_report("", Path::new("/w/staging"), Path::new("/w/out")).is_empty());
	}
}
