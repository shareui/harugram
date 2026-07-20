use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::actions::stubs_parser::graph::ImportGraph;
use crate::actions::stubs_parser::jar_index::JarIndexSet;

pub struct RequiredStubFile {
	pub path: PathBuf,
	pub fqcn: String,
}

pub struct StubResolution {
	pub required_files: Vec<RequiredStubFile>,
	pub diagnostics: Diagnostics,
}

pub struct Diagnostics {
	pub scanned_files: usize,
	pub main_fqcns: usize,
	pub closure_size: usize,
	pub skipped_main: usize,
	pub skipped_in_jar: usize,
}

pub fn resolve(main_source_root: &Path, stub_source_roots: &[&Path], jar_paths: &[String]) -> std::io::Result<StubResolution> {
	let mut roots: Vec<&Path> = vec![main_source_root];
	roots.extend(stub_source_roots);

	let graph = ImportGraph::build(&roots)?;
	let jar_index = JarIndexSet::build(jar_paths);

	let main_fqcns = graph.fqcns_declared_under(main_source_root);
	let closure = graph.transitive_closure(&main_fqcns);

	let main_fqcns_set: HashSet<String> = main_fqcns.iter().cloned().collect();

	let mut required_files = Vec::new();
	let mut seen_paths = HashSet::new();
	let mut skipped_main = 0;
	let mut skipped_in_jar = 0;
	for fqcn in &closure {
		if main_fqcns_set.contains(fqcn) {
			skipped_main += 1;
			continue;
		}
		if jar_index.contains(fqcn) {
			skipped_in_jar += 1;
			continue;
		}
		let Some(node) = graph.node(fqcn) else {
			continue;
		};
		if seen_paths.insert(node.path.clone()) {
			required_files.push(RequiredStubFile { path: node.path.clone(), fqcn: fqcn.clone() });
		}
	}

	let diagnostics = Diagnostics {
		scanned_files: graph.scanned_files(),
		main_fqcns: main_fqcns.len(),
		closure_size: closure.len(),
		skipped_main,
		skipped_in_jar,
	};

	Ok(StubResolution { required_files, diagnostics })
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;

	fn write(dir: &Path, relative: &str, contents: &str) {
		let path = dir.join(relative);
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent).unwrap();
		}
		fs::write(path, contents).unwrap();
	}

	#[test]
	fn includes_only_transitively_needed_stub_files() {
		let tmp = std::env::temp_dir().join(format!("haru_resolver_test_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		let src = tmp.join("src");
		let stubs = tmp.join("stubs");
		fs::create_dir_all(&src).unwrap();
		fs::create_dir_all(&stubs).unwrap();

		write(&src, "com/app/Main.java", "package com.app;\nimport com.sdk.Needed;\nclass Main { Needed n; }");
		write(&stubs, "com/sdk/Needed.java", "package com.sdk;\nclass Needed { }");
		write(&stubs, "com/sdk/Unused.java", "package com.sdk;\nclass Unused { }");

		let stub_roots = [stubs.as_path()];
		let resolution = resolve(&src, &stub_roots, &[]).unwrap();

		let names: Vec<String> =
			resolution.required_files.iter().filter_map(|f| f.path.file_name()).map(|n| n.to_string_lossy().to_string()).collect();

		assert!(names.contains(&"Needed.java".to_string()));
		assert!(!names.contains(&"Unused.java".to_string()));

		let _ = fs::remove_dir_all(&tmp);
	}

	#[test]
	fn skips_files_whose_class_is_already_in_a_jar() {
		let tmp = std::env::temp_dir().join(format!("haru_resolver_test2_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		let src = tmp.join("src");
		let stubs = tmp.join("stubs");
		fs::create_dir_all(&src).unwrap();
		fs::create_dir_all(&stubs).unwrap();

		write(&src, "com/app/Main.java", "package com.app;\nimport com.sdk.InJar;\nclass Main { InJar i; }");
		write(&stubs, "com/sdk/InJar.java", "package com.sdk;\nclass InJar { }");

		let jar_path = tmp.join("sdk.jar");
		build_test_jar(&jar_path, &["com/sdk/InJar.class"]);

		let stub_roots = [stubs.as_path()];
		let resolution = resolve(&src, &stub_roots, &[jar_path.to_string_lossy().to_string()]).unwrap();

		let names: Vec<String> =
			resolution.required_files.iter().filter_map(|f| f.path.file_name()).map(|n| n.to_string_lossy().to_string()).collect();

		assert!(!names.contains(&"InJar.java".to_string()));

		let _ = fs::remove_dir_all(&tmp);
	}

	#[test]
	fn excludes_main_sources_from_required_files() {
		let tmp = std::env::temp_dir().join(format!("haru_resolver_test3_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		let src = tmp.join("src");
		let stubs = tmp.join("stubs");
		fs::create_dir_all(&src).unwrap();
		fs::create_dir_all(&stubs).unwrap();

		write(&src, "com/app/Main.java", "package com.app;\nimport com.sdk.Needed;\nclass Main { Needed n; }");
		write(&stubs, "com/sdk/Needed.java", "package com.sdk;\nclass Needed { }");

		let stub_roots = [stubs.as_path()];
		let resolution = resolve(&src, &stub_roots, &[]).unwrap();

		let names: Vec<String> =
			resolution.required_files.iter().filter_map(|f| f.path.file_name()).map(|n| n.to_string_lossy().to_string()).collect();

		assert!(!names.contains(&"Main.java".to_string()));
		assert!(names.contains(&"Needed.java".to_string()));

		let _ = fs::remove_dir_all(&tmp);
	}

	fn build_test_jar(path: &Path, entries: &[&str]) {
		let file = fs::File::create(path).unwrap();
		let mut writer = zip::ZipWriter::new(file);
		let options = zip::write::SimpleFileOptions::default();
		for entry in entries {
			writer.start_file(*entry, options).unwrap();
		}
		writer.finish().unwrap();
	}
}
