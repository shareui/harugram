use std::collections::HashSet;
use std::fs::File;
use std::path::Path;

pub struct JarIndex {
	classes: HashSet<String>,
}

impl JarIndex {
	pub fn build(jar_path: &Path) -> std::io::Result<Self> {
		let file = File::open(jar_path)?;
		let archive = zip::ZipArchive::new(file).map_err(to_io_error)?;

		let mut classes = HashSet::with_capacity(archive.len());
		for name in archive.file_names() {
			let Some(fqcn) = class_entry_to_fqcn(name) else {
				continue;
			};
			classes.insert(fqcn);
		}

		Ok(Self { classes })
	}

	pub fn contains(&self, fqcn: &str) -> bool {
		self.classes.contains(fqcn)
	}
}

// "com/foo/Bar.class" -> "com.foo.Bar", "com/foo/Bar$Inner.class" -> "com.foo.Bar"
fn class_entry_to_fqcn(entry_name: &str) -> Option<String> {
	let without_ext = entry_name.strip_suffix(".class")?;
	let outer_only = without_ext.split('$').next().unwrap_or(without_ext);
	Some(outer_only.replace('/', "."))
}

fn to_io_error(err: zip::result::ZipError) -> std::io::Error {
	std::io::Error::other(err)
}

pub struct JarIndexSet {
	indices: Vec<JarIndex>,
}

impl JarIndexSet {
	pub fn build(jar_paths: &[String]) -> Self {
		let indices = jar_paths.iter().filter_map(|path| JarIndex::build(Path::new(path)).ok()).collect();
		Self { indices }
	}

	pub fn contains(&self, fqcn: &str) -> bool {
		self.indices.iter().any(|index| index.contains(fqcn))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn strips_class_extension() {
		assert_eq!(class_entry_to_fqcn("com/foo/Bar.class").as_deref(), Some("com.foo.Bar"));
	}

	#[test]
	fn strips_nested_class_suffix() {
		assert_eq!(class_entry_to_fqcn("com/foo/Bar$Inner.class").as_deref(), Some("com.foo.Bar"));
	}

	#[test]
	fn ignores_non_class_entries() {
		assert_eq!(class_entry_to_fqcn("META-INF/MANIFEST.MF"), None);
	}
}
