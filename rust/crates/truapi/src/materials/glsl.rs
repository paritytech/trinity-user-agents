//! A material body's GLSL as tokens: the words and symbols outside comments
//! and numbers, which is all the format's rules and the shader rewrites need,
//! and the functions and calls they form.

use core::ops::Range;

/// GLSL's sampling functions, which a material may not redefine.
pub const SAMPLING_FUNCTIONS: [&str; 14] = [
	"texture",
	"textureProj",
	"textureLod",
	"textureOffset",
	"texelFetch",
	"texelFetchOffset",
	"textureProjOffset",
	"textureLodOffset",
	"textureProjLod",
	"textureProjLodOffset",
	"textureGrad",
	"textureGradOffset",
	"textureProjGrad",
	"textureProjGradOffset",
];

/// Words that may come before a parameter's type.
pub const PARAMETER_QUALIFIERS: [&str; 7] =
	["in", "out", "inout", "const", "lowp", "mediump", "highp"];

/// A word or a symbol.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Token<'a> {
	/// A name or a keyword.
	Word(&'a str),
	/// Any other character but whitespace.
	Symbol(char),
}

/// The tokens of `source`. `None` when it has a preprocessor directive or a
/// comment that never ends.
pub fn tokenize(source: &str) -> Option<Vec<Token<'_>>> {
	Some(located_tokens(source)?.into_iter().map(|(_, token)| token).collect())
}

/// The tokens of `source`, each with the byte offset it starts at.
pub fn located_tokens(source: &str) -> Option<Vec<(usize, Token<'_>)>> {
	let mut tokens = Vec::new();
	let mut rest = source;
	while let Some(next) = rest.chars().next() {
		let at = source.len() - rest.len();
		if let Some(comment) = rest.strip_prefix("//") {
			rest = comment.find('\n').map_or("", |end| &comment[end..]);
		} else if let Some(comment) = rest.strip_prefix("/*") {
			rest = &comment[comment.find("*/")? + 2..];
		} else if next == '#' {
			return None;
		} else if next.is_ascii_alphabetic() || next == '_' {
			let end = rest
				.find(|character: char| !character.is_ascii_alphanumeric() && character != '_')
				.unwrap_or(rest.len());
			tokens.push((at, Token::Word(&rest[..end])));
			rest = &rest[end..];
		} else if next.is_ascii_digit() ||
			(next == '.' && rest[1..].starts_with(|character: char| character.is_ascii_digit()))
		{
			let end = rest
				.find(|character: char| !character.is_ascii_alphanumeric() && character != '.')
				.unwrap_or(rest.len());
			rest = &rest[end..];
		} else {
			if !next.is_whitespace() {
				tokens.push((at, Token::Symbol(next)));
			}
			rest = &rest[next.len_utf8()..];
		}
	}
	Some(tokens)
}

/// One text edit: `text` replaces `start..end`.
pub struct Edit {
	/// Where the replaced text starts.
	pub start: usize,
	/// Where it ends.
	pub end: usize,
	/// What replaces it.
	pub text: String,
}

/// Applies `edits` last offset first, so earlier offsets stay valid. Edits at
/// one offset are applied in the order they were made, so the last made ends
/// up first, as in Shaderlab.
pub fn apply(source: &str, mut edits: Vec<Edit>) -> String {
	edits.sort_by_key(|edit| core::cmp::Reverse(edit.start));
	let mut edited = source.to_string();
	for edit in edits {
		edited.replace_range(edit.start..edit.end, &edit.text);
	}
	edited
}

/// A function's header and, for a definition, its body.
pub struct Function<'a> {
	/// Its parameters.
	pub params: Vec<FunctionParam<'a>>,
	/// Token indexes between its braces; `None` for a declaration.
	pub body: Option<Range<usize>>,
}

/// One parameter of a function header.
pub struct FunctionParam<'a> {
	/// Its type.
	pub kind: &'a str,
	/// The byte offset its type starts at.
	pub kind_at: usize,
	/// Its name, which a declaration may leave out.
	pub name: Option<&'a str>,
	/// The byte offset after its last token.
	pub end: usize,
	/// Whether it is an array.
	pub is_array: bool,
}

/// A token and the byte offset it starts at.
pub type Located<'a> = (usize, Token<'a>);

/// The byte offset after `token`.
pub fn token_end((at, token): &Located) -> usize {
	at + match token {
		Token::Word(word) => word.len(),
		Token::Symbol(symbol) => symbol.len_utf8(),
	}
}

/// The index of the bracket closing the one at `open`.
pub fn closing(tokens: &[Located], open: usize) -> Option<usize> {
	let mut depth = 0usize;
	for (index, (_, token)) in tokens.iter().enumerate().skip(open) {
		match token {
			Token::Symbol('(' | '[' | '{') => depth += 1,
			Token::Symbol(')' | ']' | '}') => {
				depth -= 1;
				if depth == 0 {
					return Some(index);
				}
			},
			_ => {},
		}
	}
	None
}

/// `tokens` split on commas outside brackets.
pub fn split_arguments<'t, 'a>(tokens: &'t [Located<'a>]) -> Vec<&'t [Located<'a>]> {
	let mut parts = Vec::new();
	let mut depth = 0usize;
	let mut start = 0;
	for (index, (_, token)) in tokens.iter().enumerate() {
		match token {
			Token::Symbol('(' | '[' | '{') => depth += 1,
			Token::Symbol(')' | ']' | '}') => depth = depth.saturating_sub(1),
			Token::Symbol(',') if depth == 0 => {
				parts.push(&tokens[start..index]);
				start = index + 1;
			},
			_ => {},
		}
	}
	if start < tokens.len() {
		parts.push(&tokens[start..]);
	}
	parts
}

/// Top-level functions: a type (maybe an array's), a name and `(` at brace
/// depth 0.
pub fn functions<'a>(tokens: &[Located<'a>]) -> Option<Vec<Function<'a>>> {
	let mut functions = Vec::new();
	let mut depth = 0usize;
	let mut index = 0;
	while index < tokens.len() {
		match tokens[index].1 {
			Token::Symbol('{') => depth += 1,
			Token::Symbol('}') => depth = depth.saturating_sub(1),
			Token::Symbol('(')
				if depth == 0 &&
					index >= 2 && matches!(
					(tokens[index - 2].1, tokens[index - 1].1),
					(Token::Word(_) | Token::Symbol(']'), Token::Word(_))
				) =>
			{
				let close = closing(tokens, index)?;
				let params = split_arguments(&tokens[index + 1..close])
					.into_iter()
					.filter_map(function_param)
					.collect();
				let body = match tokens.get(close + 1) {
					Some((_, Token::Symbol('{'))) => {
						let end = closing(tokens, close + 1)?;
						index = end;
						Some(close + 2..end)
					},
					_ => {
						index = close;
						None
					},
				};
				functions.push(Function { params, body });
			},
			_ => {},
		}
		index += 1;
	}
	Some(functions)
}

/// `[qualifiers] type [name] [array]`. `None` for `void` or an empty list.
fn function_param<'a>(tokens: &[Located<'a>]) -> Option<FunctionParam<'a>> {
	let mut words = tokens.iter().filter_map(|(at, token)| match token {
		Token::Word(word) => Some((*at, *word)),
		Token::Symbol(_) => None,
	});
	let (kind_at, kind) = words.by_ref().find(|(_, word)| !PARAMETER_QUALIFIERS.contains(word))?;
	if kind == "void" {
		return None;
	}
	Some(FunctionParam {
		kind,
		kind_at,
		name: words.next().map(|(_, name)| name),
		end: token_end(tokens.last()?),
		is_array: tokens.iter().any(|(_, token)| *token == Token::Symbol('[')),
	})
}

/// A call's argument that is a bare name, maybe in parentheses, and the byte
/// offset of the name.
pub fn bare_name<'a>(argument: &[Located<'a>]) -> Option<(usize, &'a str)> {
	let mut inner = argument;
	while let [(_, Token::Symbol('(')), middle @ .., (_, Token::Symbol(')'))] = inner {
		inner = middle;
	}
	match inner {
		[(at, Token::Word(name))] => Some((*at, name)),
		_ => None,
	}
}

/// Every call in `body`: the index of its name and of its `)`.
pub fn calls(tokens: &[Located], body: Range<usize>) -> Option<Vec<(usize, usize)>> {
	let mut calls = Vec::new();
	for index in body {
		if let (Token::Word(_), Some((_, Token::Symbol('(')))) =
			(tokens[index].1, tokens.get(index + 1))
		{
			calls.push((index, closing(tokens, index + 1)?));
		}
	}
	Some(calls)
}
