// some

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
	Ident(String),
	Dot,
	Star,
	Semicolon,
	Less,
	Greater,
	Comma,
	Colon,
	At,
	LParen,
	RParen,
	LBrace,
	RBrace,
	Other(char),
}

pub fn tokenize(source: &str) -> Vec<Token> {
	let chars: Vec<char> = source.chars().collect();
	let mut tokens = Vec::new();
	let mut i = 0;

	while i < chars.len() {
		let c = chars[i];

		if c.is_whitespace() {
			i += 1;
			continue;
		}
		if c == '/' && chars.get(i + 1) == Some(&'/') {
			i = skip_line_comment(&chars, i);
			continue;
		}
		if c == '/' && chars.get(i + 1) == Some(&'*') {
			i = skip_block_comment(&chars, i);
			continue;
		}
		if c == '"' {
			i = skip_string(&chars, i);
			continue;
		}
		if c == '\'' {
			i = skip_char_literal(&chars, i);
			continue;
		}
		if is_ident_start(c) {
			let (ident, next) = read_ident(&chars, i);
			tokens.push(Token::Ident(ident));
			i = next;
			continue;
		}

		let token = match c {
			'.' => Token::Dot,
			'*' => Token::Star,
			';' => Token::Semicolon,
			'<' => Token::Less,
			'>' => Token::Greater,
			',' => Token::Comma,
			':' => Token::Colon,
			'@' => Token::At,
			'(' => Token::LParen,
			')' => Token::RParen,
			'{' => Token::LBrace,
			'}' => Token::RBrace,
			other => Token::Other(other),
		};
		tokens.push(token);
		i += 1;
	}

	tokens
}

fn is_ident_start(c: char) -> bool {
	c.is_alphabetic() || c == '_' || c == '$'
}

fn is_ident_continue(c: char) -> bool {
	c.is_alphanumeric() || c == '_' || c == '$'
}

fn read_ident(chars: &[char], start: usize) -> (String, usize) {
	let mut end = start + 1;
	while end < chars.len() && is_ident_continue(chars[end]) {
		end += 1;
	}
	(chars[start..end].iter().collect(), end)
}

fn skip_line_comment(chars: &[char], start: usize) -> usize {
	let mut i = start + 2;
	while i < chars.len() && chars[i] != '\n' {
		i += 1;
	}
	i
}

fn skip_block_comment(chars: &[char], start: usize) -> usize {
	let mut i = start + 2;
	let mut depth = 1;
	while i < chars.len() && depth > 0 {
		if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
			depth += 1;
			i += 2;
			continue;
		}
		if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
			depth -= 1;
			i += 2;
			continue;
		}
		i += 1;
	}
	i
}

fn skip_string(chars: &[char], start: usize) -> usize {
	let is_triple = chars.get(start + 1) == Some(&'"') && chars.get(start + 2) == Some(&'"');
	if is_triple {
		return skip_triple_string(chars, start);
	}

	let mut i = start + 1;
	while i < chars.len() {
		if chars[i] == '\\' {
			i += 2;
			continue;
		}
		if chars[i] == '"' {
			return i + 1;
		}
		i += 1;
	}
	i
}

fn skip_triple_string(chars: &[char], start: usize) -> usize {
	let mut i = start + 3;
	while i < chars.len() {
		if chars[i] == '"' && chars.get(i + 1) == Some(&'"') && chars.get(i + 2) == Some(&'"') {
			return i + 3;
		}
		i += 1;
	}
	i
}

fn skip_char_literal(chars: &[char], start: usize) -> usize {
	let mut i = start + 1;
	while i < chars.len() {
		if chars[i] == '\\' {
			i += 2;
			continue;
		}
		if chars[i] == '\'' {
			return i + 1;
		}
		i += 1;
	}
	i
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn skips_line_comment() {
		let tokens = tokenize("import Foo // bar.Baz\nclass X");
		assert!(!tokens.contains(&Token::Ident("bar".to_string())));
	}

	#[test]
	fn skips_block_comment() {
		let tokens = tokenize("/* import Bar.Baz */ import Foo");
		let idents: Vec<_> = tokens.iter().filter_map(|t| if let Token::Ident(s) = t { Some(s.as_str()) } else { None }).collect();
		assert_eq!(idents, vec!["import", "Foo"]);
	}

	#[test]
	fn skips_string_contents() {
		let tokens = tokenize("val s = \"com.foo.Bar\"");
		assert!(!tokens.iter().any(|t| matches!(t, Token::Ident(s) if s == "com")));
	}

	#[test]
	fn reads_qualified_name() {
		let tokens = tokenize("import com.foo.Bar;");
		let idents: Vec<_> = tokens.iter().filter_map(|t| if let Token::Ident(s) = t { Some(s.as_str()) } else { None }).collect();
		assert_eq!(idents, vec!["import", "com", "foo", "Bar"]);
	}
}
