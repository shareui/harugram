use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::reader::Reader;

// file-based resource dirs: subdir name -> R inner class name
const FILE_RES_TYPES: [(&str, &str); 9] =
	[("drawable", "drawable"), ("layout", "layout"), ("mipmap", "mipmap"), ("menu", "menu"), ("anim", "anim"), ("animator", "animator"), ("raw", "raw"), ("xml", "xml"), ("font", "font")];

// values/*.xml tags that declare a plain named entry -> R inner class name
const VALUE_TAG_TYPES: [(&str, &str); 7] =
	[("string", "string"), ("color", "color"), ("dimen", "dimen"), ("style", "style"), ("integer", "integer"), ("bool", "bool"), ("array", "array")];

pub fn split_stub_entry(entry: &str) -> StubEntry<'_> {
	let mut parts = entry.splitn(4, '|').map(str::trim);
	let java_path = parts.next().unwrap_or("");
	let res_path = parts.next().filter(|s| !s.is_empty());
	let gradle_path = parts.next().filter(|s| !s.is_empty());
	let gradle_properties_path = parts.next().filter(|s| !s.is_empty());
	StubEntry { java_path, res_path, gradle_path, gradle_properties_path }
}

pub struct StubEntry<'a> {
	pub java_path: &'a str,
	pub res_path: Option<&'a str>,
	pub gradle_path: Option<&'a str>,
	pub gradle_properties_path: Option<&'a str>,
}

pub fn find_r_packages(roots: &[&Path]) -> Result<BTreeSet<String>, Error> {
	find_marker_packages(roots, "R")
}

pub fn find_build_config_packages(roots: &[&Path]) -> Result<BTreeSet<String>, Error> {
	find_marker_packages(roots, "BuildConfig")
}

fn find_marker_packages(roots: &[&Path], marker: &str) -> Result<BTreeSet<String>, Error> {
	let mut packages = BTreeSet::new();
	for root in roots {
		if !root.is_dir() {
			continue;
		}
		for path in walk_dir(root).map_err(Error::Io)? {
			if !is_java_or_kotlin(&path) {
				continue;
			}
			let Ok(contents) = fs::read_to_string(&path) else {
				continue;
			};
			collect_marker_packages(&contents, marker, &mut packages);
		}
	}
	Ok(packages)
}

fn is_java_or_kotlin(path: &Path) -> bool {
	matches!(path.extension().and_then(|e| e.to_str()), Some("java") | Some("kt"))
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

fn collect_marker_packages(source: &str, marker: &str, packages: &mut BTreeSet<String>) {
	let mut own_package: Option<String> = None;
	let mut imports_marker = false;
	let dotted_marker = format!(".{marker}");
	let wildcard_marker = format!(".{marker}.*");

	for line in source.lines() {
		let trimmed = line.trim();
		if own_package.is_none() {
			if let Some(rest) = trimmed.strip_prefix("package ") {
				own_package = Some(rest.trim().trim_end_matches(';').trim().to_string());
				continue;
			}
		}
		let Some(rest) = trimmed.strip_prefix("import ") else {
			continue;
		};
		let imported = rest.trim().trim_end_matches(';').trim();
		if let Some(package) = imported.strip_suffix(&wildcard_marker).or_else(|| imported.strip_suffix(&dotted_marker)) {
			packages.insert(package.to_string());
			imports_marker = true;
		}
	}

	// a file that references "<marker>." but never imports it relies on same-package resolution
	if !imports_marker && references_bare_word(source, marker) {
		if let Some(package) = own_package {
			packages.insert(package);
		}
	}
}

fn references_bare_word(source: &str, word: &str) -> bool {
	let needle = format!("{word}.");
	let bytes = source.as_bytes();
	let mut i = 0;
	while let Some(offset) = source[i..].find(&needle) {
		let start = i + offset;
		let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
		if before_ok {
			return true;
		}
		i = start + needle.len();
	}
	false
}

fn is_ident_byte(b: u8) -> bool {
	b.is_ascii_alphanumeric() || b == b'_'
}

pub struct BuildConfigField {
	pub java_type: String,
	pub name: String,
	pub value: String,
}

pub fn parse_gradle_properties(path: &Path) -> Result<BTreeMap<String, String>, Error> {
	let contents = fs::read_to_string(path).map_err(Error::Io)?;
	let mut props = BTreeMap::new();
	for line in contents.lines() {
		let trimmed = line.trim();
		if trimmed.is_empty() || trimmed.starts_with('#') {
			continue;
		}
		let Some((key, value)) = trimmed.split_once('=') else {
			continue;
		};
		props.insert(key.trim().to_string(), value.trim().to_string());
	}
	Ok(props)
}

pub fn parse_build_gradle(path: &Path, properties: &BTreeMap<String, String>) -> Result<GradleConfig, Error> {
	let contents = fs::read_to_string(path).map_err(Error::Io)?;

	let namespace = find_quoted_arg(&contents, "namespace");
	let application_id = find_quoted_arg(&contents, "applicationId").or_else(|| namespace.clone());
	let build_type = find_first_build_type_name(&contents);

	let mut fields: BTreeMap<String, BuildConfigField> = BTreeMap::new();
	for line in contents.lines() {
		let Some(field) = parse_build_config_field_line(line, properties) else {
			continue;
		};
		fields.entry(field.name.clone()).or_insert(field);
	}

	Ok(GradleConfig { application_id, build_type, fields: fields.into_values().collect() })
}

pub struct GradleConfig {
	pub application_id: Option<String>,
	pub build_type: Option<String>,
	pub fields: Vec<BuildConfigField>,
}

fn find_quoted_arg(source: &str, key: &str) -> Option<String> {
	for line in source.lines() {
		let trimmed = line.trim();
		let trimmed = trimmed.strip_prefix("defaultConfig.").unwrap_or(trimmed);
		let Some(rest) = trimmed.strip_prefix(key) else {
			continue;
		};
		let rest = rest.trim_start();
		let rest = rest.strip_prefix('=').unwrap_or(rest).trim_start();
		if !rest.starts_with('\'') && !rest.starts_with('"') {
			continue;
		}
		if let Some(value) = extract_quoted(rest) {
			return Some(value);
		}
	}
	None
}

fn find_first_build_type_name(source: &str) -> Option<String> {
	let start = source.find("buildTypes")?;
	let after = &source[start..];
	for line in after.lines().skip(1) {
		let trimmed = line.trim();
		if let Some(name) = trimmed.strip_suffix('{') {
			let name = name.trim();
			if !name.is_empty() && name.chars().all(|c| is_ident_byte(c as u8)) {
				return Some(name.to_string());
			}
		}
	}
	None
}

fn parse_build_config_field_line(line: &str, properties: &BTreeMap<String, String>) -> Option<BuildConfigField> {
	let trimmed = line.trim();
	let rest = trimmed.strip_prefix("buildConfigField")?.trim_start();
	let mut parts = split_top_level_commas(rest);
	if parts.len() < 3 {
		return None;
	}
	let java_type = extract_quoted(parts.remove(0).trim())?;
	let name = extract_quoted(parts.remove(0).trim())?;
	let value_expr = parts.join(",");
	let value = resolve_field_value(&java_type, value_expr.trim(), properties);
	Some(BuildConfigField { java_type, name, value })
}

fn split_top_level_commas(source: &str) -> Vec<String> {
	let mut parts = Vec::new();
	let mut current = String::new();
	let mut in_quotes = false;
	let mut quote_char = '"';
	let mut chars = source.chars();

	while let Some(c) = chars.next() {
		if in_quotes {
			current.push(c);
			if c == '\\' {
				if let Some(escaped) = chars.next() {
					current.push(escaped);
				}
				continue;
			}
			if c == quote_char {
				in_quotes = false;
			}
			continue;
		}
		match c {
			'"' | '\'' => {
				in_quotes = true;
				quote_char = c;
				current.push(c);
			}
			',' => {
				parts.push(std::mem::take(&mut current));
			}
			_ => current.push(c),
		}
	}
	parts.push(current);
	parts
}

fn resolve_field_value(java_type: &str, expr: &str, properties: &BTreeMap<String, String>) -> String {
	if let Some(resolved) = resolve_string_concat(expr, properties) {
		return format!("\"{}\"", escape_java_string(&resolved));
	}
	let trimmed = expr.trim();
	if trimmed == "true" || trimmed == "false" {
		return trimmed.to_string();
	}
	if trimmed.chars().all(|c| c.is_ascii_digit() || c == '-') && !trimmed.is_empty() {
		return trimmed.to_string();
	}
	if let Some(value) = properties.get(trimmed) {
		return format_literal(java_type, value);
	}
	default_for_type(java_type)
}

fn resolve_string_concat(expr: &str, properties: &BTreeMap<String, String>) -> Option<String> {
	if !expr.contains('+') {
		return None;
	}
	let mut result = String::new();
	for segment in expr.split('+') {
		let segment = segment.trim();
		if let Some(literal) = extract_quoted(segment) {
			result.push_str(&literal);
			continue;
		}
		let value = properties.get(segment)?;
		result.push_str(value);
	}
	Some(result)
}

// escapes '"' and '\' so a resolved value can be safely embedded in a generated Java string literal
fn escape_java_string(raw: &str) -> String {
	raw.chars().flat_map(|c| if c == '"' || c == '\\' { vec!['\\', c] } else { vec![c] }).collect()
}

fn format_literal(java_type: &str, raw_value: &str) -> String {
	match java_type {
		"String" => format!("\"{}\"", escape_java_string(raw_value)),
		_ => raw_value.to_string(),
	}
}

fn default_for_type(java_type: &str) -> String {
	match java_type {
		"String" => "\"\"".to_string(),
		"boolean" => "false".to_string(),
		"int" | "long" | "short" | "byte" => "0".to_string(),
		"float" | "double" => "0.0".to_string(),
		_ => "null".to_string(),
	}
}

// extracts the content of a leading 'quoted' or "quoted" literal, honoring backslash escapes
// (e.g. "\"" is a one-character string containing a quote), ignoring anything after the closing quote
fn extract_quoted(source: &str) -> Option<String> {
	let source = source.trim();
	let quote = source.chars().next()?;
	if quote != '\'' && quote != '"' {
		return None;
	}
	let rest = &source[quote.len_utf8()..];
	let mut chars = rest.chars();
	let mut result = String::new();
	while let Some(c) = chars.next() {
		if c == '\\' {
			if let Some(escaped) = chars.next() {
				result.push(escaped);
			}
			continue;
		}
		if c == quote {
			return Some(result);
		}
		result.push(c);
	}
	None
}

// renders "package com.example;\n\npublic final class BuildConfig { ... }"
pub fn render_build_config(package: &str, config: &GradleConfig) -> String {
	let mut out = String::new();
	out.push_str(&format!("package {package};\n\n"));
	out.push_str("public final class BuildConfig {\n");
	out.push_str("\tpublic static final boolean DEBUG = false;\n");
	let application_id = config.application_id.as_deref().unwrap_or(package);
	out.push_str(&format!("\tpublic static final String APPLICATION_ID = \"{application_id}\";\n"));
	let build_type = config.build_type.as_deref().unwrap_or("debug");
	out.push_str(&format!("\tpublic static final String BUILD_TYPE = \"{build_type}\";\n"));
	out.push_str("\tpublic static final String FLAVOR = \"\";\n");
	for field in &config.fields {
		out.push_str(&format!("\tpublic static final {} {} = {};\n", field.java_type, field.name, field.value));
	}
	out.push_str("}\n");
	out
}

// writes the generated BuildConfig.java under cache_dir/<package/as/path>/BuildConfig.java
pub fn write_build_config(cache_dir: &Path, package: &str, config: &GradleConfig) -> Result<PathBuf, Error> {
	let mut dir = cache_dir.to_path_buf();
	for segment in package.split('.') {
		dir.push(segment);
	}
	fs::create_dir_all(&dir).map_err(Error::Io)?;

	let path = dir.join("BuildConfig.java");
	let contents = render_build_config(package, config);
	fs::write(&path, contents).map_err(Error::Io)?;
	Ok(path)
}

#[derive(Debug)]
pub enum Error {
	Io(std::io::Error),
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Io(err) => write!(f, "{err}"),
		}
	}
}

// inner class name -> sorted entry names, e.g. "id" -> ["button_ok", "text_title"]
#[derive(Default)]
pub struct ResourceIndex {
	entries: BTreeMap<&'static str, Vec<String>>,
}

impl ResourceIndex {
	fn add(&mut self, class_name: &'static str, entry_name: String) {
		let names = self.entries.entry(class_name).or_default();
		if !names.contains(&entry_name) {
			names.push(entry_name);
		}
	}

	pub fn merge(&mut self, other: ResourceIndex) {
		for (class_name, names) in other.entries {
			for name in names {
				self.add(class_name, name);
			}
		}
	}

	pub fn is_empty(&self) -> bool {
		self.entries.is_empty()
	}
}

// scans a single res/ directory into a ResourceIndex, ignoring files it does not recognize
pub fn scan_res_dir(res_dir: &Path) -> Result<ResourceIndex, Error> {
	let mut index = ResourceIndex::default();
	let entries = fs::read_dir(res_dir).map_err(Error::Io)?;

	for entry in entries {
		let entry = entry.map_err(Error::Io)?;
		let path = entry.path();
		if !path.is_dir() {
			continue;
		}
		let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
			continue;
		};
		let qualifier_base = dir_name.split('-').next().unwrap_or(dir_name);

		if qualifier_base == "values" {
			scan_values_dir(&path, &mut index)?;
			continue;
		}

		let Some((_, class_name)) = FILE_RES_TYPES.iter().find(|(prefix, _)| *prefix == qualifier_base) else {
			continue;
		};
		scan_file_res_dir(&path, class_name, &mut index)?;
	}

	Ok(index)
}

fn scan_file_res_dir(dir: &Path, class_name: &'static str, index: &mut ResourceIndex) -> Result<(), Error> {
	for entry in fs::read_dir(dir).map_err(Error::Io)? {
		let entry = entry.map_err(Error::Io)?;
		let path = entry.path();
		if !path.is_file() {
			continue;
		}
		let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
			continue;
		};
		index.add(class_name, sanitize_name(stem));
	}
	Ok(())
}

fn scan_values_dir(dir: &Path, index: &mut ResourceIndex) -> Result<(), Error> {
	for entry in fs::read_dir(dir).map_err(Error::Io)? {
		let entry = entry.map_err(Error::Io)?;
		let path = entry.path();
		if path.extension().and_then(|e| e.to_str()) != Some("xml") {
			continue;
		}
		let contents = fs::read_to_string(&path).map_err(Error::Io)?;
		parse_values_xml(&contents, index);
	}
	Ok(())
}

// walks top-level <string name=..>, <item type=.. name=..>, <declare-styleable> child attrs, etc
fn parse_values_xml(xml: &str, index: &mut ResourceIndex) {
	let mut reader = Reader::from_str(xml);
	reader.config_mut().trim_text(true);

	loop {
		match reader.read_event() {
			Ok(Event::Start(tag)) | Ok(Event::Empty(tag)) => {
				let tag_name = String::from_utf8_lossy(tag.name().as_ref()).to_string();
				handle_values_tag(&tag_name, &tag, index);
			}
			Ok(Event::Eof) => break,
			Err(_) => break,
			_ => {}
		}
	}
}

fn handle_values_tag(tag_name: &str, tag: &quick_xml::events::BytesStart, index: &mut ResourceIndex) {
	if tag_name == "item" {
		let Some(name) = attr_value(tag, "name") else {
			return;
		};
		let item_type = attr_value(tag, "type").unwrap_or_else(|| "id".to_string());
		let class_name = value_class_for(&item_type).unwrap_or("id");
		index.add(class_name, sanitize_name(&name));
		return;
	}

	if tag_name == "plurals" {
		if let Some(name) = attr_value(tag, "name") {
			index.add("plurals", sanitize_name(&name));
		}
		return;
	}

	if tag_name == "attr" {
		if let Some(name) = attr_value(tag, "name") {
			index.add("attr", sanitize_name(&name));
		}
		return;
	}

	let Some((_, class_name)) = VALUE_TAG_TYPES.iter().find(|(tag, _)| *tag == tag_name) else {
		return;
	};
	let Some(name) = attr_value(tag, "name") else {
		return;
	};
	index.add(class_name, sanitize_name(&name));
}

fn value_class_for(item_type: &str) -> Option<&'static str> {
	VALUE_TAG_TYPES.iter().chain(std::iter::once(&("id", "id"))).find(|(tag, _)| *tag == item_type).map(|(_, class_name)| *class_name)
}

fn attr_value(tag: &quick_xml::events::BytesStart, key: &str) -> Option<String> {
	tag.attributes().flatten().find(|attr| attr.key.as_ref() == key.as_bytes()).map(|attr| String::from_utf8_lossy(&attr.value).to_string())
}

// android resource names use '.' and '-' as separators, java identifiers cannot
fn sanitize_name(raw: &str) -> String {
	raw.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect()
}

// deterministic stub id: stable across builds for the same package + class + name, not aapt-compatible
fn stub_id(package: &str, class_name: &str, entry_name: &str) -> u32 {
	let mut hash: u32 = 0x811c_9dc5;
	for byte in format!("{package}.{class_name}.{entry_name}").bytes() {
		hash ^= byte as u32;
		hash = hash.wrapping_mul(0x0100_0193);
	}
	0x7f00_0000 | (hash & 0x00ff_ffff)
}

// renders "package com.example;\n\npublic final class R { ... }"
pub fn render(package: &str, index: &ResourceIndex) -> String {
	let mut out = String::new();
	out.push_str(&format!("package {package};\n\n"));
	out.push_str("public final class R {\n");
	for (class_name, names) in &index.entries {
		out.push_str(&format!("\tpublic static final class {class_name} {{\n"));
		for name in names {
			let id = stub_id(package, class_name, name);
			out.push_str(&format!("\t\tpublic static final int {name} = 0x{id:08x};\n"));
		}
		out.push_str("\t}\n");
	}
	out.push_str("}\n");
	out
}

// writes the generated R.java under cache_dir/<package/as/path>/R.java, returns the written path
pub fn write_r_java(cache_dir: &Path, package: &str, index: &ResourceIndex) -> Result<PathBuf, Error> {
	let mut dir = cache_dir.to_path_buf();
	for segment in package.split('.') {
		dir.push(segment);
	}
	fs::create_dir_all(&dir).map_err(Error::Io)?;

	let path = dir.join("R.java");
	let contents = render(package, index);
	fs::write(&path, contents).map_err(Error::Io)?;
	Ok(path)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn write(dir: &Path, relative: &str, contents: &str) {
		let path = dir.join(relative);
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent).unwrap();
		}
		fs::write(path, contents).unwrap();
	}

	#[test]
	fn scans_file_based_resources_by_name() {
		let tmp = std::env::temp_dir().join(format!("haru_res_gen_test_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		write(&tmp, "drawable/ic_launcher.png", "");
		write(&tmp, "layout/activity_main.xml", "<LinearLayout />");

		let index = scan_res_dir(&tmp).unwrap();
		assert_eq!(index.entries.get("drawable").unwrap(), &vec!["ic_launcher".to_string()]);
		assert_eq!(index.entries.get("layout").unwrap(), &vec!["activity_main".to_string()]);

		let _ = fs::remove_dir_all(&tmp);
	}

	#[test]
	fn scans_values_xml_entries() {
		let tmp = std::env::temp_dir().join(format!("haru_res_gen_test2_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		write(
			&tmp,
			"values/strings.xml",
			"<resources>\n<string name=\"app_name\">Haru</string>\n<item type=\"id\" name=\"button_ok\" />\n</resources>",
		);

		let index = scan_res_dir(&tmp).unwrap();
		assert_eq!(index.entries.get("string").unwrap(), &vec!["app_name".to_string()]);
		assert_eq!(index.entries.get("id").unwrap(), &vec!["button_ok".to_string()]);

		let _ = fs::remove_dir_all(&tmp);
	}

	#[test]
	fn sanitizes_dashed_names() {
		assert_eq!(sanitize_name("ic-launcher.round"), "ic_launcher_round");
	}

	#[test]
	fn splits_stub_entry_with_res_path() {
		let entry = split_stub_entry("./stubs/java|./stubs/res");
		assert_eq!(entry.java_path, "./stubs/java");
		assert_eq!(entry.res_path, Some("./stubs/res"));
		assert_eq!(entry.gradle_path, None);
		assert_eq!(entry.gradle_properties_path, None);

		let entry = split_stub_entry("./stubs/java");
		assert_eq!(entry.java_path, "./stubs/java");
		assert_eq!(entry.res_path, None);
		assert_eq!(entry.gradle_path, None);
		assert_eq!(entry.gradle_properties_path, None);
	}

	#[test]
	fn splits_stub_entry_with_gradle_path() {
		let entry = split_stub_entry("./stubs/java|./stubs/res|./stubs/build.gradle");
		assert_eq!(entry.java_path, "./stubs/java");
		assert_eq!(entry.res_path, Some("./stubs/res"));
		assert_eq!(entry.gradle_path, Some("./stubs/build.gradle"));
		assert_eq!(entry.gradle_properties_path, None);
	}

	#[test]
	fn splits_stub_entry_with_empty_res_but_gradle_path() {
		let entry = split_stub_entry("./stubs/java||./stubs/build.gradle");
		assert_eq!(entry.java_path, "./stubs/java");
		assert_eq!(entry.res_path, None);
		assert_eq!(entry.gradle_path, Some("./stubs/build.gradle"));
		assert_eq!(entry.gradle_properties_path, None);
	}

	#[test]
	fn splits_stub_entry_with_all_four_parts() {
		let entry = split_stub_entry("./stubs/java|./stubs/res|./stubs/build.gradle|./stubs/gradle.properties");
		assert_eq!(entry.java_path, "./stubs/java");
		assert_eq!(entry.res_path, Some("./stubs/res"));
		assert_eq!(entry.gradle_path, Some("./stubs/build.gradle"));
		assert_eq!(entry.gradle_properties_path, Some("./stubs/gradle.properties"));
	}

	#[test]
	fn finds_package_from_explicit_r_import() {
		let tmp = std::env::temp_dir().join(format!("haru_res_gen_test3_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		write(&tmp, "org/telegram/ui/LaunchActivity.java", "package org.telegram.ui;\nimport org.telegram.messenger.R;\nclass LaunchActivity { int x = R.string.app_name; }");

		let packages = find_r_packages(&[tmp.as_path()]).unwrap();
		assert!(packages.contains("org.telegram.messenger"));

		let _ = fs::remove_dir_all(&tmp);
	}

	#[test]
	fn finds_own_package_on_same_package_r_usage() {
		let tmp = std::env::temp_dir().join(format!("haru_res_gen_test4_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		write(&tmp, "org/telegram/messenger/BuildVars.java", "package org.telegram.messenger;\nclass BuildVars { int x = R.string.app_name; }");

		let packages = find_r_packages(&[tmp.as_path()]).unwrap();
		assert!(packages.contains("org.telegram.messenger"));

		let _ = fs::remove_dir_all(&tmp);
	}

	#[test]
	fn ignores_files_that_never_reference_r() {
		let tmp = std::env::temp_dir().join(format!("haru_res_gen_test5_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		write(&tmp, "com/app/Plain.java", "package com.app;\nclass Plain { int x = 1; }");

		let packages = find_r_packages(&[tmp.as_path()]).unwrap();
		assert!(packages.is_empty());

		let _ = fs::remove_dir_all(&tmp);
	}

	#[test]
	fn stub_ids_are_stable_across_calls() {
		let a = stub_id("com.app", "id", "button_ok");
		let b = stub_id("com.app", "id", "button_ok");
		assert_eq!(a, b);
	}

	#[test]
	fn render_produces_valid_class_shape() {
		let mut index = ResourceIndex::default();
		index.add("id", "button_ok".to_string());
		let source = render("com.app", &index);
		assert!(source.contains("package com.app;"));
		assert!(source.contains("public static final class id {"));
		assert!(source.contains("public static final int button_ok"));
	}

	#[test]
	fn parses_gradle_properties_ignoring_comments() {
		let tmp = std::env::temp_dir().join(format!("haru_res_gen_gp_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		write(&tmp, "gradle.properties", "# a comment\nAPP_VERSION_NAME=12.9.0\n\nAPP_VERSION_CODE=6966\n");

		let props = parse_gradle_properties(&tmp.join("gradle.properties")).unwrap();
		assert_eq!(props.get("APP_VERSION_NAME"), Some(&"12.9.0".to_string()));
		assert_eq!(props.get("APP_VERSION_CODE"), Some(&"6966".to_string()));

		let _ = fs::remove_dir_all(&tmp);
	}

	#[test]
	fn extracts_namespace_as_application_id_when_no_application_id_present() {
		let tmp = std::env::temp_dir().join(format!("haru_res_gen_bg1_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		write(&tmp, "build.gradle", "android {\n    namespace 'org.telegram.messenger'\n}\n");

		let props = BTreeMap::new();
		let config = parse_build_gradle(&tmp.join("build.gradle"), &props).unwrap();
		assert_eq!(config.application_id.as_deref(), Some("org.telegram.messenger"));

		let _ = fs::remove_dir_all(&tmp);
	}

	#[test]
	fn finds_first_build_type_name() {
		let tmp = std::env::temp_dir().join(format!("haru_res_gen_bg2_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		write(&tmp, "build.gradle", "android {\n    buildTypes {\n        debug {\n            minifyEnabled false\n        }\n        release {\n        }\n    }\n}\n");

		let props = BTreeMap::new();
		let config = parse_build_gradle(&tmp.join("build.gradle"), &props).unwrap();
		assert_eq!(config.build_type.as_deref(), Some("debug"));

		let _ = fs::remove_dir_all(&tmp);
	}

	#[test]
	fn resolves_simple_literal_build_config_fields() {
		let tmp = std::env::temp_dir().join(format!("haru_res_gen_bg3_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		write(
			&tmp,
			"build.gradle",
			"android {\n    buildTypes {\n        debug {\n            buildConfigField \"boolean\", \"BUNDLE\", \"false\"\n            buildConfigField \"int\", \"VERSION_NUM\", \"7\"\n        }\n    }\n}\n",
		);

		let props = BTreeMap::new();
		let config = parse_build_gradle(&tmp.join("build.gradle"), &props).unwrap();
		let bundle = config.fields.iter().find(|f| f.name == "BUNDLE").unwrap();
		assert_eq!(bundle.value, "false");
		let version_num = config.fields.iter().find(|f| f.name == "VERSION_NUM").unwrap();
		assert_eq!(version_num.value, "7");

		let _ = fs::remove_dir_all(&tmp);
	}

	#[test]
	fn resolves_quote_wrapped_property_concatenation() {
		// real-world case from TMessagesProj: buildConfigField "String", "BUILD_VERSION_STRING", "\"" + APP_VERSION_NAME + "\""
		// the quotes are literal characters in the Groovy string, not string delimiters
		let tmp = std::env::temp_dir().join(format!("haru_res_gen_bg4_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		write(
			&tmp,
			"build.gradle",
			"android {\n    buildTypes {\n        debug {\n            buildConfigField \"String\", \"BUILD_VERSION_STRING\", \"\\\"\" + APP_VERSION_NAME + \"\\\"\"\n        }\n    }\n}\n",
		);

		let mut props = BTreeMap::new();
		props.insert("APP_VERSION_NAME".to_string(), "12.9.0".to_string());
		let config = parse_build_gradle(&tmp.join("build.gradle"), &props).unwrap();
		let field = config.fields.iter().find(|f| f.name == "BUILD_VERSION_STRING").unwrap();
		assert_eq!(field.value, "\"\\\"12.9.0\\\"\"");

		let _ = fs::remove_dir_all(&tmp);
	}

	#[test]
	fn falls_back_to_type_default_for_unresolvable_expressions() {
		let tmp = std::env::temp_dir().join(format!("haru_res_gen_bg5_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		write(
			&tmp,
			"build.gradle",
			"android {\n    buildTypes {\n        debug {\n            buildConfigField \"String\", \"BETA_URL\", \"\\\"\" + getProps(\"BETA_PRIVATE_URL\") + \"\\\"\"\n            buildConfigField \"boolean\", \"BUILD_HOST_IS_WINDOWS\", isWindows\n        }\n    }\n}\n",
		);

		let props = BTreeMap::new();
		let config = parse_build_gradle(&tmp.join("build.gradle"), &props).unwrap();
		let beta_url = config.fields.iter().find(|f| f.name == "BETA_URL").unwrap();
		assert_eq!(beta_url.value, "\"\"");
		let is_windows = config.fields.iter().find(|f| f.name == "BUILD_HOST_IS_WINDOWS").unwrap();
		assert_eq!(is_windows.value, "false");

		let _ = fs::remove_dir_all(&tmp);
	}

	#[test]
	fn render_build_config_produces_valid_class_shape() {
		let config = GradleConfig {
			application_id: Some("org.telegram.messenger".to_string()),
			build_type: Some("debug".to_string()),
			fields: vec![BuildConfigField { java_type: "boolean".to_string(), name: "BUNDLE".to_string(), value: "false".to_string() }],
		};
		let source = render_build_config("org.telegram.messenger", &config);
		assert!(source.contains("package org.telegram.messenger;"));
		assert!(source.contains("public static final String APPLICATION_ID = \"org.telegram.messenger\";"));
		assert!(source.contains("public static final String BUILD_TYPE = \"debug\";"));
		assert!(source.contains("public static final boolean BUNDLE = false;"));
	}

	#[test]
	fn finds_build_config_package_from_import() {
		let tmp = std::env::temp_dir().join(format!("haru_res_gen_bc_import_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		write(&tmp, "org/telegram/ui/LaunchActivity.java", "package org.telegram.ui;\nimport org.telegram.messenger.BuildConfig;\nclass LaunchActivity { boolean d = BuildConfig.DEBUG; }");

		let packages = find_build_config_packages(&[tmp.as_path()]).unwrap();
		assert!(packages.contains("org.telegram.messenger"));

		let _ = fs::remove_dir_all(&tmp);
	}
}
