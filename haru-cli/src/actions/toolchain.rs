use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
	Kotlinc,
	Javac,
	D8,
}

impl Tool {
	fn binary_name(self) -> &'static str {
		match self {
			Self::Kotlinc => "kotlinc",
			Self::Javac => "javac",
			Self::D8 => "d8",
		}
	}

	fn config_field(self) -> &'static str {
		match self {
			Self::Kotlinc => "kotlinc",
			Self::Javac => "javac",
			Self::D8 => "d8",
		}
	}

	fn env_var(self) -> &'static str {
		match self {
			Self::Kotlinc => "KOTLINC_PATH",
			Self::Javac => "JAVAC_PATH",
			Self::D8 => "D8_PATH",
		}
	}
}

// order: standard android studio paths, then config.json, then env, then PATH
pub fn locate(tool: Tool) -> Option<PathBuf> {
	if let Some(path) = locate_standard(tool) {
		return Some(path);
	}
	if let Some(path) = locate_from_config(tool) {
		return Some(path);
	}
	if let Some(path) = locate_from_env(tool) {
		return Some(path);
	}
	locate_from_path(tool)
}

fn locate_from_config(tool: Tool) -> Option<PathBuf> {
	let root = config::load_raw()?;
	let raw = root.get(tool.config_field())?.as_str()?;
	let path = PathBuf::from(raw);
	path.exists().then_some(path)
}

fn locate_from_env(tool: Tool) -> Option<PathBuf> {
	let raw = std::env::var(tool.env_var()).ok()?;
	let path = PathBuf::from(raw);
	path.exists().then_some(path)
}

fn locate_from_path(tool: Tool) -> Option<PathBuf> {
	let finder = if cfg!(windows) { "where" } else { "which" };
	let output = Command::new(finder).arg(tool.binary_name()).output().ok()?;
	if !output.status.success() {
		return None;
	}
	let stdout = String::from_utf8_lossy(&output.stdout);
	let first_line = stdout.lines().next()?.trim();
	if first_line.is_empty() {
		return None;
	}
	Some(PathBuf::from(first_line))
}

fn locate_standard(tool: Tool) -> Option<PathBuf> {
	match tool {
		Tool::Kotlinc | Tool::Javac => locate_standard_binary(tool),
		Tool::D8 => locate_standard_d8(),
	}
}

fn locate_standard_binary(tool: Tool) -> Option<PathBuf> {
	let ext = if cfg!(windows) { ".exe".to_string() } else { String::new() };
	let sub_path = if cfg!(windows) { format!("bin\\{}{ext}", tool.binary_name()) } else { format!("bin/{}", tool.binary_name()) };

	for root in android_studio_roots() {
		let candidate = root.join(&sub_path);
		if candidate.exists() {
			return Some(candidate);
		}
	}
	None
}

fn android_studio_roots() -> Vec<PathBuf> {
	let mut roots = Vec::new();
	let Some(home) = dirs::home_dir() else {
		return roots;
	};

	if cfg!(windows) {
		roots.push(PathBuf::from("C:\\Program Files\\Android\\Android Studio\\jbr"));
		roots.push(home.join("AppData\\Local\\Programs\\Android Studio\\jbr"));
	} else if cfg!(target_os = "macos") {
		roots.push(PathBuf::from("/Applications/Android Studio.app/Contents/jbr/Contents/Home"));
		roots.push(home.join("Applications/Android Studio.app/Contents/jbr/Contents/Home"));
	} else {
		roots.push(PathBuf::from("/opt/android-studio/jbr"));
		roots.push(home.join(".local/share/JetBrains/Toolbox/apps/android-studio/jbr"));
		roots.push(home.join("android-studio/jbr"));
	}

	roots
}

fn locate_standard_d8() -> Option<PathBuf> {
	let binary_name = if cfg!(windows) { "d8.bat" } else { "d8" };

	for sdk_root in android_sdk_roots() {
		let build_tools = sdk_root.join("build-tools");
		let Some(latest) = latest_versioned_dir(&build_tools) else {
			continue;
		};
		let candidate = latest.join(binary_name);
		if candidate.exists() {
			return Some(candidate);
		}
	}
	None
}

fn android_sdk_roots() -> Vec<PathBuf> {
	let mut roots = Vec::new();
	let Some(home) = dirs::home_dir() else {
		return roots;
	};

	if cfg!(windows) {
		roots.push(home.join("AppData\\Local\\Android\\Sdk"));
	} else if cfg!(target_os = "macos") {
		roots.push(home.join("Library/Android/sdk"));
	} else {
		roots.push(home.join("Android/Sdk"));
	}

	roots
}

fn latest_versioned_dir(build_tools: &Path) -> Option<PathBuf> {
	let entries = std::fs::read_dir(build_tools).ok()?;
	entries
		.filter_map(|entry| entry.ok())
		.filter(|entry| entry.path().is_dir())
		.max_by(|a, b| compare_version_names(&a.file_name().to_string_lossy(), &b.file_name().to_string_lossy()))
		.map(|entry| entry.path())
}

fn compare_version_names(a: &str, b: &str) -> std::cmp::Ordering {
	let parse = |raw: &str| -> Vec<u64> { raw.split('.').filter_map(|part| part.parse::<u64>().ok()).collect() };
	parse(a).cmp(&parse(b))
}
