use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const APP_DIR_NAME: &str = "haru";
const CONFIG_FILE_NAME: &str = "config.json";

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
	#[serde(default = "default_first_opening")]
	pub first_opening: bool,

	#[serde(default = "default_theme")]
	pub theme: String,
}

impl Default for Config {
	fn default() -> Self {
		Self { first_opening: default_first_opening(), theme: default_theme() }
	}
}

fn default_first_opening() -> bool {
	true
}

fn default_theme() -> String {
	"ash violet".to_string()
}

// windows: %APPDATA%\haru
// macos:   ~/Library/Application Support/haru
// linux:   ~/.config/haru (or $XDG_CONFIG_HOME/haru)
pub fn dir() -> Option<PathBuf> {
	dirs::config_dir().map(|base| base.join(APP_DIR_NAME))
}

pub fn path() -> Option<PathBuf> {
	dir().map(|dir| dir.join(CONFIG_FILE_NAME))
}

// creates the config directory and a default config.json if they do not exist yet,
// leaves an existing config file untouched
pub fn ensure() -> io::Result<()> {
	let Some(dir) = dir() else {
		return Err(io::Error::other("could not resolve platform config directory"));
	};
	fs::create_dir_all(&dir)?;

	let config_path = dir.join(CONFIG_FILE_NAME);
	if config_path.exists() {
		return Ok(());
	}

	let default_config = Config::default();
	let json = serde_json::to_string_pretty(&default_config).map_err(io::Error::other)?;
	fs::write(config_path, json)
}

pub fn load() -> Config {
	let Some(path) = path() else {
		return Config::default();
	};
	let Ok(contents) = fs::read_to_string(path) else {
		return Config::default();
	};
	serde_json::from_str(&contents).unwrap_or_default()
}

pub fn save(config: &Config) -> io::Result<()> {
	let Some(path) = path() else {
		return Err(io::Error::other("could not resolve platform config directory"));
	};
	if let Some(dir) = path.parent() {
		fs::create_dir_all(dir)?;
	}
	let json = serde_json::to_string_pretty(config).map_err(io::Error::other)?;
	fs::write(path, json)
}

pub fn save_theme(theme: &str) -> io::Result<()> {
	let mut cfg = load();
	cfg.theme = if theme.is_empty() { default_theme() } else { theme.to_string() };
	save(&cfg)
}

pub fn load_raw() -> Option<serde_json::Value> {
	let path = path()?;
	let contents = fs::read_to_string(path).ok()?;
	serde_json::from_str(&contents).ok()
}

pub fn save_raw(value: &serde_json::Value) -> io::Result<()> {
	let Some(path) = path() else {
		return Err(io::Error::other("could not resolve platform config directory"));
	};
	if let Some(dir) = path.parent() {
		fs::create_dir_all(dir)?;
	}
	let json = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
	fs::write(path, json)
}
