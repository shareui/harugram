use crate::actions::stubs_parser::lexer::Token;

#[derive(Debug, Default)]
pub struct FileFacts {
	pub package: Option<String>,
	pub imports: Vec<String>,
	pub type_refs: Vec<String>,
}

pub fn extract(tokens: &[Token]) -> FileFacts {
	let mut facts = FileFacts::default();
	let mut i = 0;

	while i < tokens.len() {
		match &tokens[i] {
			Token::Ident(word) if word == "package" => {
				let (name, next) = read_dotted_path(tokens, i + 1);
				facts.package = name;
				i = next;
			}
			Token::Ident(word) if word == "import" => {
				let (name, next) = read_import_path(tokens, i + 1);
				if let Some(name) = name {
					facts.imports.push(name);
				}
				i = next;
			}
			Token::Ident(word) if is_type_position_keyword(word) => {
				i += 1;
				i = collect_type_refs_until_boundary(tokens, i, &mut facts.type_refs);
			}
			Token::Colon | Token::Less | Token::At => {
				i += 1;
				i = collect_type_refs_until_boundary(tokens, i, &mut facts.type_refs);
			}
			Token::Ident(word) if is_capitalized(word) && !after_type_declaration_keyword(tokens, i) && starts_declaration(tokens, i) => {
				i = collect_type_refs_until_boundary(tokens, i, &mut facts.type_refs);
			}
			_ => i += 1,
		}
	}

	facts
}

fn is_type_position_keyword(word: &str) -> bool {
	matches!(word, "extends" | "implements" | "new" | "is" | "as")
}

fn read_dotted_path(tokens: &[Token], mut i: usize) -> (Option<String>, usize) {
	let mut parts = Vec::new();
	let mut expect_ident = true;
	loop {
		match tokens.get(i) {
			Some(Token::Ident(word)) if expect_ident => {
				parts.push(word.clone());
				expect_ident = false;
				i += 1;
			}
			Some(Token::Dot) if !parts.is_empty() && !expect_ident => {
				expect_ident = true;
				i += 1;
			}
			_ => break,
		}
	}
	if parts.is_empty() {
		return (None, i);
	}
	(Some(parts.join(".")), i)
}

fn read_import_path(tokens: &[Token], mut i: usize) -> (Option<String>, usize) {
	let mut parts = Vec::new();
	let mut expect_ident = true;
	loop {
		match tokens.get(i) {
			Some(Token::Ident(word)) if expect_ident => {
				parts.push(word.clone());
				expect_ident = false;
				i += 1;
			}
			Some(Token::Dot) if !parts.is_empty() && !expect_ident => {
				if tokens.get(i + 1) == Some(&Token::Star) {
					parts.push("*".to_string());
					i += 2;
					break;
				}
				expect_ident = true;
				i += 1;
			}
			_ => break,
		}
	}
	if parts.is_empty() {
		return (None, i);
	}
	(Some(parts.join(".")), i)
}

fn collect_type_refs_until_boundary(tokens: &[Token], mut i: usize, out: &mut Vec<String>) -> usize {
	let boundary_depth_start = 0;
	let mut angle_depth = boundary_depth_start;

	while i < tokens.len() {
		match &tokens[i] {
			Token::Ident(word) => {
				if is_capitalized(word) {
					let (name, next) = read_dotted_path(tokens, i);
					if let Some(name) = name {
						out.push(name);
					}
					i = next;
					continue;
				}
				i += 1;
			}
			Token::Less => {
				angle_depth += 1;
				i += 1;
			}
			Token::Greater => {
				if angle_depth == 0 {
					break;
				}
				angle_depth -= 1;
				i += 1;
			}
			Token::Comma if angle_depth > 0 => i += 1,
			Token::Semicolon | Token::LBrace | Token::RBrace => break,
			Token::LParen if angle_depth == 0 => break,
			_ => i += 1,
		}
	}

	i
}

fn is_capitalized(word: &str) -> bool {
	word.chars().next().is_some_and(char::is_uppercase)
}

fn after_type_declaration_keyword(tokens: &[Token], i: usize) -> bool {
	i > 0 && matches!(tokens.get(i - 1), Some(Token::Ident(word)) if matches!(word.as_str(), "class" | "interface" | "enum" | "record"))
}

fn starts_declaration(tokens: &[Token], i: usize) -> bool {
	let Some(Token::Ident(_)) = tokens.get(i) else {
		return false;
	};
	let mut j = i + 1;
	while tokens.get(j) == Some(&Token::Dot) {
		let Some(Token::Ident(_)) = tokens.get(j + 1) else {
			return false;
		};
		j += 2;
	}
	if tokens.get(j) == Some(&Token::Less) {
		let Some(next) = skip_generic_args(tokens, j) else {
			return false;
		};
		j = next;
	}
	matches!(tokens.get(j), Some(Token::Ident(_)))
}

fn skip_generic_args(tokens: &[Token], less_index: usize) -> Option<usize> {
	let mut depth = 0;
	let mut j = less_index;
	loop {
		match tokens.get(j) {
			Some(Token::Less) => {
				depth += 1;
				j += 1;
			}
			Some(Token::Greater) => {
				depth -= 1;
				j += 1;
				if depth == 0 {
					return Some(j);
				}
			}
			Some(Token::Ident(_) | Token::Dot | Token::Comma) => j += 1,
			_ => return None,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::actions::stubs_parser::lexer::tokenize;

	#[test]
	fn extracts_package_and_imports() {
		let tokens = tokenize("package com.foo;\nimport com.bar.Baz;\nimport com.qux.*;");
		let facts = extract(&tokens);
		assert_eq!(facts.package.as_deref(), Some("com.foo"));
		assert_eq!(facts.imports, vec!["com.bar.Baz", "com.qux.*"]);
	}

	#[test]
	fn kotlin_package_without_semicolon_does_not_swallow_following_import() {
		let tokens = tokenize("package com.foo\n\nimport com.bar.Baz\n\nfun main() { }");
		let facts = extract(&tokens);
		assert_eq!(facts.package.as_deref(), Some("com.foo"));
		assert_eq!(facts.imports, vec!["com.bar.Baz"]);
	}

	#[test]
	fn extracts_extends_and_implements() {
		let tokens = tokenize("class Foo extends Bar implements Baz, Qux { }");
		let facts = extract(&tokens);
		assert!(facts.type_refs.contains(&"Bar".to_string()));
		assert!(facts.type_refs.contains(&"Baz".to_string()));
		assert!(facts.type_refs.contains(&"Qux".to_string()));
	}

	#[test]
	fn extracts_kotlin_colon_type() {
		let tokens = tokenize("class Foo : Bar() { val x: Baz = Baz() }");
		let facts = extract(&tokens);
		assert!(facts.type_refs.contains(&"Bar".to_string()));
		assert!(facts.type_refs.contains(&"Baz".to_string()));
	}

	#[test]
	fn extracts_generic_type_args() {
		let tokens = tokenize("val list: List<Foo> = ArrayList<Foo>()");
		let facts = extract(&tokens);
		assert!(facts.type_refs.contains(&"List".to_string()));
		assert!(facts.type_refs.contains(&"Foo".to_string()));
	}

	#[test]
	fn extracts_annotation() {
		let tokens = tokenize("@MyAnnotation class Foo { }");
		let facts = extract(&tokens);
		assert!(facts.type_refs.contains(&"MyAnnotation".to_string()));
	}

	#[test]
	fn ignores_lowercase_identifiers() {
		let tokens = tokenize("class Foo extends bar { }");
		let facts = extract(&tokens);
		assert!(!facts.type_refs.contains(&"bar".to_string()));
	}

	#[test]
	fn extracts_field_declaration_without_keyword() {
		let tokens = tokenize("class Foo { public WebMetadataCache.WebMetadata meta; }");
		let facts = extract(&tokens);
		assert!(facts.type_refs.contains(&"WebMetadataCache.WebMetadata".to_string()));
	}

	#[test]
	fn extracts_method_return_type_without_keyword() {
		let tokens = tokenize("class Foo { private MathSpan mathSpanAt(float x) { } }");
		let facts = extract(&tokens);
		assert!(facts.type_refs.contains(&"MathSpan".to_string()));
	}

	#[test]
	fn does_not_treat_declared_class_name_as_type_ref() {
		let tokens = tokenize("public class Foo extends Bar { }");
		let facts = extract(&tokens);
		assert!(!facts.type_refs.contains(&"Foo".to_string()));
		assert!(facts.type_refs.contains(&"Bar".to_string()));
	}
}
