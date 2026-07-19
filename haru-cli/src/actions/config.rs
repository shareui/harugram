use serde_json::{Map, Value};

use crate::config;

#[derive(Debug)]
pub enum Error {
	ConfigNotFound,
	FieldNotFound { field: String, suggestion: Option<String> },
	FieldAlreadyExists(String),
	TypeMismatch { expected: &'static str, got: &'static str },
	CannotParseField(String),
	Io(std::io::Error),
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::ConfigNotFound => write!(f, "Config not found"),
			Self::FieldNotFound { field: _, suggestion: Some(suggestion) } => {
				write!(f, "Field with this name does not exist, maybe you meant \"{suggestion}\"")
			}
			Self::FieldNotFound { field, suggestion: None } => write!(f, "Field \"{field}\" does not exist"),
			Self::FieldAlreadyExists(field) => write!(f, "Field \"{field}\" already exists"),
			Self::TypeMismatch { expected, got } => write!(f, "Value type missmatch, expected {expected} got {got}"),
			Self::CannotParseField(field) => write!(f, "Cannot parse field \"{field}\""),
			Self::Io(err) => write!(f, "{err}"),
		}
	}
}

pub fn set(field: &str, raw_value: &str) -> Result<(), Error> {
	let mut root = load_root()?;
	let segments = split_path(field)?;

	let existing = navigate(&root, &segments).ok_or_else(|| not_found(&root, field))?;
	let new_value = parse_value(raw_value);
	check_type_match(existing, &new_value)?;

	assign(&mut root, &segments, new_value)?;
	config::save_raw(&root).map_err(Error::Io)
}

pub fn new(field: &str, raw_value: &str) -> Result<(), Error> {
	let mut root = load_root()?;
	let segments = split_path(field)?;

	if navigate(&root, &segments).is_some() {
		return Err(Error::FieldAlreadyExists(field.to_string()));
	}

	let new_value = parse_value(raw_value);
	assign(&mut root, &segments, new_value)?;
	config::save_raw(&root).map_err(Error::Io)
}

pub fn del(field: &str) -> Result<(), Error> {
	let mut root = load_root()?;
	let segments = split_path(field)?;

	if navigate(&root, &segments).is_none() {
		return Err(not_found(&root, field));
	}

	remove(&mut root, &segments)?;
	config::save_raw(&root).map_err(Error::Io)
}

fn load_root() -> Result<Value, Error> {
	config::load_raw().ok_or(Error::ConfigNotFound)
}

fn split_path(field: &str) -> Result<Vec<String>, Error> {
	if field.is_empty() {
		return Err(Error::CannotParseField(field.to_string()));
	}
	let segments: Vec<String> = field.split('.').map(str::to_string).collect();
	if segments.iter().any(|segment| segment.is_empty()) {
		return Err(Error::CannotParseField(field.to_string()));
	}
	Ok(segments)
}

fn navigate<'a>(root: &'a Value, segments: &[String]) -> Option<&'a Value> {
	let mut current = root;
	for segment in segments {
		current = current.as_object()?.get(segment)?;
	}
	Some(current)
}

// walks to the parent object of the final segment, creating intermediate objects as needed
fn assign(root: &mut Value, segments: &[String], value: Value) -> Result<(), Error> {
	let (last, parents) = segments.split_last().expect("segments is never empty");
	let mut current = root;
	for segment in parents {
		let object = current.as_object_mut().ok_or_else(|| Error::CannotParseField(segments.join(".")))?;
		current = object.entry(segment.clone()).or_insert_with(|| Value::Object(Map::new()));
	}
	let object = current.as_object_mut().ok_or_else(|| Error::CannotParseField(segments.join(".")))?;
	object.insert(last.clone(), value);
	Ok(())
}

fn remove(root: &mut Value, segments: &[String]) -> Result<(), Error> {
	let (last, parents) = segments.split_last().expect("segments is never empty");
	let mut current = root;
	for segment in parents {
		current = current.as_object_mut().and_then(|object| object.get_mut(segment)).ok_or_else(|| Error::CannotParseField(segments.join(".")))?;
	}
	let object = current.as_object_mut().ok_or_else(|| Error::CannotParseField(segments.join(".")))?;
	object.remove(last);
	Ok(())
}

// bool/int/float parsed first, anything else stays a string
fn parse_value(raw: &str) -> Value {
	if let Ok(value) = raw.parse::<bool>() {
		return Value::Bool(value);
	}
	if let Ok(value) = raw.parse::<i64>() {
		return Value::Number(value.into());
	}
	if let Ok(value) = raw.parse::<f64>() {
		if let Some(number) = serde_json::Number::from_f64(value) {
			return Value::Number(number);
		}
	}
	Value::String(raw.to_string())
}

fn check_type_match(existing: &Value, new_value: &Value) -> Result<(), Error> {
	let expected = type_name(existing);
	let got = type_name(new_value);
	if expected == got {
		return Ok(());
	}
	Err(Error::TypeMismatch { expected, got })
}

fn type_name(value: &Value) -> &'static str {
	match value {
		Value::Null => "null",
		Value::Bool(_) => "bool",
		Value::Number(number) if number.is_i64() || number.is_u64() => "int",
		Value::Number(_) => "float",
		Value::String(_) => "string",
		Value::Array(_) => "array",
		Value::Object(_) => "object",
	}
}

fn not_found(root: &Value, field: &str) -> Error {
	let suggestion = closest_field(root, field);
	Error::FieldNotFound { field: field.to_string(), suggestion }
}

fn closest_field(root: &Value, field: &str) -> Option<String> {
	let mut paths = Vec::new();
	collect_paths(root, String::new(), &mut paths);

	paths.into_iter().map(|path| (levenshtein(field, &path), path)).min_by_key(|(distance, _)| *distance).map(|(_, path)| path)
}

fn collect_paths(value: &Value, prefix: String, paths: &mut Vec<String>) {
	let Value::Object(map) = value else {
		return;
	};
	for (key, child) in map {
		let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
		paths.push(path.clone());
		collect_paths(child, path, paths);
	}
}

fn levenshtein(a: &str, b: &str) -> usize {
	let a: Vec<char> = a.chars().collect();
	let b: Vec<char> = b.chars().collect();
	let mut row: Vec<usize> = (0..=b.len()).collect();

	for i in 1..=a.len() {
		let mut prev_diag = row[0];
		row[0] = i;
		for j in 1..=b.len() {
			let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
			let deletion = row[j] + 1;
			let insertion = row[j - 1] + 1;
			let substitution = prev_diag + cost;
			prev_diag = row[j];
			row[j] = deletion.min(insertion).min(substitution);
		}
	}
	row[b.len()]
}
