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
}
