//! The format's rules for the GLSL body, checked on its tokens. Shaderlab's
//! reference also parses the GLSL, so a body it refuses only by parsing, such
//! as one with a syntax error, passes here and fails the device's compiler.

use crate::materials::glsl::{PARAMETER_QUALIFIERS, SAMPLING_FUNCTIONS, Token, tokenize};

const INPUTS: [&str; 8] = [
	"r_input",
	"r_base",
	"r_noise",
	"sl_time",
	"sl_tilt",
	"sl_orientation",
	"sl_rotation",
	"sl_resolution",
];

const INTERFACE_KEYWORDS: [&str; 7] =
	["uniform", "layout", "attribute", "varying", "buffer", "shared", "precision"];

const RESERVED_PREFIXES: [&str; 2] = ["gl_", "_shaderlab_"];

const INPUT_PREFIXES: [&str; 2] = ["sl_", "r_"];

/// Whether `body` keeps every body rule and declares `vec4 material(vec2)`,
/// both as Shaderlab reads it and as the GLSL compiler reads it. The compiler
/// first joins a line ending in a backslash to the next and Shaderlab does
/// not, so either reading alone would let a banned word be split across lines.
pub fn follows_body_rules(body: &str) -> bool {
	follows_rules_as_read(body) && follows_rules_as_read(&body.replace("\\\n", ""))
}

/// Struct names are types, which Shaderlab does not hold to the name rules.
fn follows_rules_as_read(body: &str) -> bool {
	let Some(tokens) = tokenize(body) else {
		return false;
	};
	let struct_names: Vec<&str> = tokens
		.windows(2)
		.filter_map(|pair| match pair {
			[Token::Word("struct"), Token::Word(name)] => Some(*name),
			_ => None,
		})
		.collect();
	let words_allowed = tokens.iter().all(|token| match token {
		Token::Word(word) => struct_names.contains(word) || word_allowed(word),
		Token::Symbol(_) => true,
	});
	words_allowed && entry_point_declared(&tokens)
}

/// Whether `word` may appear in a body: no interface keyword, no reserved
/// backend name, and a name shaped like a renderer input must be one.
fn word_allowed(word: &str) -> bool {
	let starts_with_any = |prefixes: &[&str]| prefixes.iter().any(|prefix| word.starts_with(prefix));
	if INTERFACE_KEYWORDS.contains(&word) || starts_with_any(&RESERVED_PREFIXES) {
		return false;
	}
	INPUTS.contains(&word) || !starts_with_any(&INPUT_PREFIXES)
}

/// Every top-level function named `material` must have the entry point's
/// signature, at least one must exist, and none may be named `main` or
/// redefine a sampling function.
fn entry_point_declared(tokens: &[Token]) -> bool {
	let mut declared = false;
	let mut braces = 0usize;
	let mut parens = 0usize;
	for (at, token) in tokens.iter().enumerate() {
		match token {
			Token::Symbol('{') => braces += 1,
			Token::Symbol('}') => braces = braces.saturating_sub(1),
			Token::Symbol('(') => {
				if braces == 0 &&
					parens == 0 && at >= 2 &&
					let (Token::Word(result), Token::Word(name)) =
						(tokens[at - 2], tokens[at - 1])
				{
					match name {
						"main" => return false,
						name if SAMPLING_FUNCTIONS.contains(&name) => return false,
						"material" if result == "vec4" && entry_parameter(&tokens[at + 1..]) => {
							declared = true
						},
						"material" => return false,
						_ => {},
					}
				}
				parens += 1;
			},
			Token::Symbol(')') => parens = parens.saturating_sub(1),
			_ => {},
		}
	}
	declared
}

/// `[qualifiers] vec2 [name] )`, the tokens after the entry point's `(`.
fn entry_parameter(tokens: &[Token]) -> bool {
	let mut rest = tokens.iter().skip_while(
		|token| matches!(token, Token::Word(word) if PARAMETER_QUALIFIERS.contains(word)),
	);
	if rest.next() != Some(&Token::Word("vec2")) {
		return false;
	}
	match rest.next() {
		Some(Token::Symbol(')')) => true,
		Some(Token::Word(_)) => rest.next() == Some(&Token::Symbol(')')),
		_ => false,
	}
}
