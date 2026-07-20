use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use crate::actions::stubs_parser::file_model::{self, FileFacts};
use crate::actions::stubs_parser::lexer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
	Kotlin,
	Java,
}

pub struct GraphNode {
	pub path: PathBuf,
	pub lang: Lang,
	pub fqcn: String,
	pub facts: FileFacts,
}

pub struct ImportGraph {
	// fqcn -> source file that declares it
	declared_by: HashMap<String, PathBuf>,
	edges: HashMap<String, HashSet<String>>,
	nodes: HashMap<String, GraphNode>,
}

impl ImportGraph {
	pub fn build(roots: &[&Path]) -> std::io::Result<Self> {
		let mut nodes = HashMap::new();
		let mut declared_by = HashMap::new();

		for root in roots {
			for path in walk_dir(root)? {
				let Some(lang) = lang_of(&path) else {
					continue;
				};
				let Ok(source) = fs::read_to_string(&path) else {
					continue;
				};
				let tokens = lexer::tokenize(&source);
				let facts = file_model::extract(&tokens);
				let type_name = type_name_from_path(&path);
				let fqcn = match &facts.package {
					Some(package) => format!("{package}.{type_name}"),
					None => type_name.clone(),
				};

				declared_by.insert(fqcn.clone(), path.clone());
				nodes.insert(fqcn.clone(), GraphNode { path: path.clone(), lang, fqcn, facts });
			}
		}

		let mut edges = HashMap::new();
		for (fqcn, node) in &nodes {
			let resolved = resolve_dependencies(node, &declared_by);
			edges.insert(fqcn.clone(), resolved);
		}

		Ok(Self { declared_by, edges, nodes })
	}

	pub fn node(&self, fqcn: &str) -> Option<&GraphNode> {
		self.nodes.get(fqcn)
	}

	pub fn contains(&self, fqcn: &str) -> bool {
		self.declared_by.contains_key(fqcn)
	}

	pub fn scanned_files(&self) -> usize {
		self.nodes.len()
	}

	pub fn fqcns_declared_under(&self, root: &Path) -> Vec<String> {
		let canonical_root = root.canonicalize().ok();
		self.declared_by
			.iter()
			.filter(|(_, path)| match (&canonical_root, path.canonicalize()) {
				(Some(root), Ok(path)) => path.starts_with(root),
				_ => false,
			})
			.map(|(fqcn, _)| fqcn.clone())
			.collect()
	}

	pub fn transitive_closure(&self, start: &[String]) -> HashSet<String> {
		let mut visited: HashSet<String> = HashSet::new();
		let mut queue: VecDeque<String> = VecDeque::new();

		for fqcn in start {
			if self.declared_by.contains_key(fqcn) && visited.insert(fqcn.clone()) {
				queue.push_back(fqcn.clone());
			}
		}

		while let Some(current) = queue.pop_front() {
			let Some(deps) = self.edges.get(&current) else {
				continue;
			};
			for dep in deps {
				if visited.insert(dep.clone()) {
					queue.push_back(dep.clone());
				}
			}
		}

		visited
	}
}

fn resolve_dependencies(node: &GraphNode, declared_by: &HashMap<String, PathBuf>) -> HashSet<String> {
	let mut resolved = HashSet::new();

	for import in &node.facts.imports {
		if let Some(fqcn) = resolve_import(import, declared_by) {
			resolved.insert(fqcn);
		}
	}

	let own_package = node.facts.package.as_deref();
	for type_ref in &node.facts.type_refs {
		if let Some(fqcn) = resolve_type_ref(type_ref, own_package, &node.facts.imports, declared_by) {
			resolved.insert(fqcn);
		}
	}

	resolved
}

fn resolve_import(import: &str, declared_by: &HashMap<String, PathBuf>) -> Option<String> {
	if let Some(package) = import.strip_suffix(".*") {
		let _ = package;
		return None;
	}
	declared_by.contains_key(import).then(|| import.to_string())
}

fn resolve_type_ref(type_ref: &str, own_package: Option<&str>, imports: &[String], declared_by: &HashMap<String, PathBuf>) -> Option<String> {
	if type_ref.contains('.') {
		return declared_by.contains_key(type_ref).then(|| type_ref.to_string());
	}

	for import in imports {
		if import.ends_with(&format!(".{type_ref}")) && declared_by.contains_key(import) {
			return Some(import.clone());
		}
	}

	if let Some(package) = own_package {
		let candidate = format!("{package}.{type_ref}");
		if declared_by.contains_key(&candidate) {
			return Some(candidate);
		}
	}

	for import in imports {
		let Some(package) = import.strip_suffix(".*") else {
			continue;
		};
		let candidate = format!("{package}.{type_ref}");
		if declared_by.contains_key(&candidate) {
			return Some(candidate);
		}
	}

	None
}

fn lang_of(path: &Path) -> Option<Lang> {
	match path.extension().and_then(|e| e.to_str()) {
		Some("kt") => Some(Lang::Kotlin),
		Some("java") => Some(Lang::Java),
		_ => None,
	}
}

fn type_name_from_path(path: &Path) -> String {
	path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
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
	fn resolves_direct_import_transitively() {
		let tmp = std::env::temp_dir().join(format!("haru_graph_test_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		fs::create_dir_all(&tmp).unwrap();

		write(&tmp, "com/app/Main.java", "package com.app;\nimport com.util.Helper;\nclass Main { Helper h; }");
		write(&tmp, "com/util/Helper.java", "package com.util;\nimport com.util.Deep;\nclass Helper { Deep d; }");
		write(&tmp, "com/util/Deep.java", "package com.util;\nclass Deep { }");
		write(&tmp, "com/util/Unrelated.java", "package com.util;\nclass Unrelated { }");

		let graph = ImportGraph::build(&[&tmp]).unwrap();
		let closure = graph.transitive_closure(&["com.app.Main".to_string()]);

		assert!(closure.contains("com.app.Main"));
		assert!(closure.contains("com.util.Helper"));
		assert!(closure.contains("com.util.Deep"));
		assert!(!closure.contains("com.util.Unrelated"));

		let _ = fs::remove_dir_all(&tmp);
	}

	#[test]
	fn resolves_same_package_reference_without_import() {
		let tmp = std::env::temp_dir().join(format!("haru_graph_test2_{}", std::process::id()));
		let _ = fs::remove_dir_all(&tmp);
		fs::create_dir_all(&tmp).unwrap();

		write(&tmp, "com/app/Main.kt", "package com.app\nclass Main : Base() { }");
		write(&tmp, "com/app/Base.kt", "package com.app\nopen class Base { }");

		let graph = ImportGraph::build(&[&tmp]).unwrap();
		let closure = graph.transitive_closure(&["com.app.Main".to_string()]);

		assert!(closure.contains("com.app.Base"));

		let _ = fs::remove_dir_all(&tmp);
	}
}
