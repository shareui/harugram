use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde::Deserialize;

use crate::actions::maven::error::Error;

#[derive(Debug, Deserialize, Default)]
#[serde(rename = "project")]
pub struct RawPom {
	#[serde(rename = "groupId")]
	pub group_id: Option<String>,
	#[serde(rename = "artifactId")]
	pub artifact_id: Option<String>,
	pub version: Option<String>,
	pub packaging: Option<String>,
	pub parent: Option<RawParent>,
	#[serde(rename = "dependencyManagement")]
	pub dependency_management: Option<RawDependencyBlock>,
	pub dependencies: Option<RawDependencyBlock>,
}

#[derive(Debug, Deserialize)]
pub struct RawParent {
	#[serde(rename = "groupId")]
	pub group_id: String,
	#[serde(rename = "artifactId")]
	pub artifact_id: String,
	pub version: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct RawDependencyBlock {
	#[serde(rename = "dependency", default)]
	pub dependency: Vec<RawDependency>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RawDependency {
	#[serde(rename = "groupId")]
	pub group_id: String,
	#[serde(rename = "artifactId")]
	pub artifact_id: String,
	pub version: Option<String>,
	pub scope: Option<String>,
	pub optional: Option<String>,
	#[serde(rename = "type")]
	pub kind: Option<String>,
}

// pom after property substitution and parent inheritance have been applied
pub struct ResolvedPom {
	pub group_id: String,
	pub artifact_id: String,
	pub version: String,
	pub packaging: String,
	pub properties: HashMap<String, String>,
	pub dependency_management: HashMap<(String, String), String>,
	pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone)]
pub struct Dependency {
	pub group_id: String,
	pub artifact_id: String,
	pub version: Option<String>,
	pub scope: String,
	pub optional: bool,
}

impl Dependency {
	pub fn needed_at_runtime(&self) -> bool {
		!self.optional && matches!(self.scope.as_str(), "compile" | "runtime")
	}
}

pub fn parse(xml: &str, coordinate_label: &str) -> Result<RawPom, Error> {
	quick_xml::de::from_str(xml).map_err(|err| Error::PomInvalid { coordinate: coordinate_label.to_string(), reason: err.to_string() })
}

pub fn parse_properties(xml: &str) -> HashMap<String, String> {
	let mut reader = Reader::from_str(xml);
	reader.config_mut().trim_text(true);

	let mut properties = HashMap::new();
	let mut depth_in_properties: Option<u32> = None;
	let mut current_tag: Option<String> = None;
	let mut buf = Vec::new();

	loop {
		match reader.read_event_into(&mut buf) {
			Ok(Event::Start(e)) => {
				let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
				if name == "properties" {
					depth_in_properties = Some(0);
				} else if let Some(depth) = depth_in_properties {
					if depth == 0 {
						current_tag = Some(name);
					}
					depth_in_properties = Some(depth + 1);
				}
			}
			Ok(Event::Text(e)) => {
				if let Some(tag) = &current_tag {
					if let Ok(text) = e.decode() {
						properties.insert(tag.clone(), text.into_owned());
					}
				}
			}
			Ok(Event::End(e)) => {
				let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
				if name == "properties" {
					depth_in_properties = None;
					current_tag = None;
				} else if let Some(depth) = depth_in_properties {
					if depth <= 1 {
						current_tag = None;
					}
					depth_in_properties = Some(depth.saturating_sub(1));
				}
			}
			Ok(Event::Eof) => break,
			Err(_) => break,
			_ => {}
		}
		buf.clear();
	}

	properties
}

pub fn resolve(raw: RawPom, xml: &str, parent: Option<&ResolvedPom>, coordinate_label: &str) -> Result<ResolvedPom, Error> {
	let mut properties: HashMap<String, String> = HashMap::new();
	if let Some(parent) = parent {
		properties.extend(parent.properties.clone());
	}
	properties.extend(parse_properties(xml));

	let group_id = raw.group_id.or_else(|| raw.parent.as_ref().map(|p| p.group_id.clone())).ok_or_else(|| missing_field(coordinate_label, "groupId"))?;
	let version = raw.version.or_else(|| raw.parent.as_ref().map(|p| p.version.clone())).ok_or_else(|| missing_field(coordinate_label, "version"))?;
	let artifact_id = raw.artifact_id.ok_or_else(|| missing_field(coordinate_label, "artifactId"))?;

	properties.insert("project.groupId".to_string(), group_id.clone());
	properties.insert("project.artifactId".to_string(), artifact_id.clone());
	properties.insert("project.version".to_string(), version.clone());

	let mut dependency_management: HashMap<(String, String), String> = HashMap::new();
	if let Some(parent) = parent {
		dependency_management.extend(parent.dependency_management.clone());
	}
	if let Some(block) = &raw.dependency_management {
		for dependency in &block.dependency {
			let Some(version) = &dependency.version else { continue };
			let resolved_version = substitute(version, &properties);
			dependency_management.insert((dependency.group_id.clone(), dependency.artifact_id.clone()), resolved_version);
		}
	}

	let mut dependencies = Vec::new();
	if let Some(block) = &raw.dependencies {
		for raw_dependency in &block.dependency {
			dependencies.push(resolve_dependency(raw_dependency, &properties, &dependency_management));
		}
	}

	Ok(ResolvedPom {
		group_id: substitute(&group_id, &properties),
		artifact_id,
		version: substitute(&version, &properties),
		packaging: raw.packaging.unwrap_or_else(|| "jar".to_string()),
		properties,
		dependency_management,
		dependencies,
	})
}

fn resolve_dependency(raw: &RawDependency, properties: &HashMap<String, String>, dependency_management: &HashMap<(String, String), String>) -> Dependency {
	let group_id = substitute(&raw.group_id, properties);
	let artifact_id = substitute(&raw.artifact_id, properties);

	let version = raw
		.version
		.as_ref()
		.map(|v| substitute(v, properties))
		.or_else(|| dependency_management.get(&(group_id.clone(), artifact_id.clone())).cloned());

	let scope = raw.scope.clone().unwrap_or_else(|| "compile".to_string());
	let optional = raw.optional.as_deref().map(|v| v == "true").unwrap_or(false);

	Dependency { group_id, artifact_id, version, scope, optional }
}

fn missing_field(coordinate_label: &str, field: &'static str) -> Error {
	Error::PomInvalid { coordinate: coordinate_label.to_string(), reason: format!("missing required field \"{field}\"") }
}

fn substitute(raw: &str, properties: &HashMap<String, String>) -> String {
	if !raw.contains("${") {
		return raw.to_string();
	}

	let mut result = String::with_capacity(raw.len());
	let mut rest = raw;
	while let Some(start) = rest.find("${") {
		result.push_str(&rest[..start]);
		let after_start = &rest[start + 2..];
		let Some(end) = after_start.find('}') else {
			result.push_str(&rest[start..]);
			rest = "";
			break;
		};
		let property_name = &after_start[..end];
		match properties.get(property_name) {
			Some(value) => result.push_str(value),
			None => {
				result.push_str("${");
				result.push_str(property_name);
				result.push('}');
			}
		}
		rest = &after_start[end + 1..];
	}
	result.push_str(rest);
	result
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn substitutes_known_property() {
		let mut properties = HashMap::new();
		properties.insert("foo.version".to_string(), "1.2.3".to_string());
		assert_eq!(substitute("${foo.version}", &properties), "1.2.3");
	}

	#[test]
	fn leaves_unknown_property_untouched() {
		let properties = HashMap::new();
		assert_eq!(substitute("${unknown}", &properties), "${unknown}");
	}

	#[test]
	fn dependency_needed_at_runtime_excludes_test_and_optional() {
		let compile = Dependency { group_id: "g".into(), artifact_id: "a".into(), version: Some("1".into()), scope: "compile".into(), optional: false };
		let test = Dependency { scope: "test".into(), ..compile.clone() };
		let optional = Dependency { optional: true, ..compile.clone() };
		assert!(compile.needed_at_runtime());
		assert!(!test.needed_at_runtime());
		assert!(!optional.needed_at_runtime());
	}

	#[test]
	fn parses_properties_block() {
		let xml = r#"<project>
			<properties>
				<kotlin.version>1.9.0</kotlin.version>
				<other>value</other>
			</properties>
		</project>"#;
		let properties = parse_properties(xml);
		assert_eq!(properties.get("kotlin.version").map(String::as_str), Some("1.9.0"));
		assert_eq!(properties.get("other").map(String::as_str), Some("value"));
	}

	#[test]
	fn parses_minimal_pom() {
		let xml = r#"<project>
			<groupId>com.example</groupId>
			<artifactId>lib</artifactId>
			<version>1.0.0</version>
			<dependencies>
				<dependency>
					<groupId>com.example</groupId>
					<artifactId>other</artifactId>
					<version>2.0.0</version>
				</dependency>
			</dependencies>
		</project>"#;
		let raw = parse(xml, "com.example:lib:1.0.0").unwrap();
		let resolved = resolve(raw, xml, None, "com.example:lib:1.0.0").unwrap();
		assert_eq!(resolved.group_id, "com.example");
		assert_eq!(resolved.dependencies.len(), 1);
		assert_eq!(resolved.dependencies[0].version.as_deref(), Some("2.0.0"));
	}
}
