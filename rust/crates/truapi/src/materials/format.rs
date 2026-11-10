//! Material text in format version 1, read as Shaderlab's reference parser
//! reads it, so every host accepts and refuses the same materials.
//!
//! Text that is valid but spelled another way is refused, so each material
//! has one ID. JSON numbers are spelled as JavaScript writes them, and blank
//! means JavaScript's whitespace, because the reference is JavaScript.

use derive_more::Display;
use serde_json::{Map, Value};
use truapi::latest::{ImageRef, MaterialFailure, Parameter};

use self::body::follows_body_rules;
use super::MAX_MATERIAL_BYTES;

mod body;

/// Longest GLSL body, in UTF-16 units as JavaScript counts a string.
const MAX_BODY_UNITS: usize = 32_768;

/// Most parameters one material declares.
const MAX_PARAMS: usize = 32;

const HEADER: &str = "// @material 1";
const REQUIRES: &str = "// @requires ";
const CONTROL: &str = "// @control ";
const PARAM_KEYS: [&str; 7] = ["name", "label", "type", "default", "min", "max", "presentation"];
const PRESENTATION_KEYS: [&str; 5] = ["version", "group", "help", "unit", "step"];

/// Why material text was refused, displayed as the format's own code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum FormatError {
	/// Not UTF-8, has a byte-order mark or NUL, or is too long.
	#[display("invalid-encoding")]
	InvalidEncoding,
	/// The header is missing or names a version this host does not read.
	#[display("unsupported-version")]
	UnsupportedVersion,
	/// The material requires a feature this host does not read.
	#[display("unsupported-feature")]
	UnsupportedFeature,
	/// A parameter line breaks a parameter rule.
	#[display("invalid-parameter")]
	InvalidParameter,
	/// The body breaks a body rule, or a directive is out of place.
	#[display("invalid-source")]
	InvalidSource,
	/// The text is valid but not in its one canonical spelling.
	#[display("non-canonical")]
	NonCanonical,
}

impl From<FormatError> for MaterialFailure {
	fn from(error: FormatError) -> Self {
		match error {
			FormatError::UnsupportedVersion | FormatError::UnsupportedFeature => Self::Unsupported,
			FormatError::InvalidEncoding |
			FormatError::InvalidParameter |
			FormatError::InvalidSource |
			FormatError::NonCanonical => Self::InvalidMaterial,
		}
	}
}

/// The type of a declared parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
	/// `float`
	Float,
	/// `vec2`
	Vec2,
	/// `vec3`
	Vec3,
	/// `vec4`
	Vec4,
	/// `samplerFloat`
	SamplerFloat,
	/// `samplerVec2`
	SamplerVec2,
	/// `samplerVec3`
	SamplerVec3,
	/// `samplerVec4`
	SamplerVec4,
}

impl ParamKind {
	fn parse(name: &str) -> Option<Self> {
		Some(match name {
			"float" => Self::Float,
			"vec2" => Self::Vec2,
			"vec3" => Self::Vec3,
			"vec4" => Self::Vec4,
			"samplerFloat" => Self::SamplerFloat,
			"samplerVec2" => Self::SamplerVec2,
			"samplerVec3" => Self::SamplerVec3,
			"samplerVec4" => Self::SamplerVec4,
			_ => return None,
		})
	}

	fn is_sampler(self) -> bool {
		matches!(
			self,
			Self::SamplerFloat | Self::SamplerVec2 | Self::SamplerVec3 | Self::SamplerVec4
		)
	}

	/// Whether `default` is a value of this number type: a number for `float`,
	/// an array of as many numbers as components for a vector.
	fn accepts_default(self, default: &Value) -> bool {
		let components = match self {
			Self::Float => return gpu_float(default),
			Self::Vec2 => 2,
			Self::Vec3 => 3,
			Self::Vec4 => 4,
			Self::SamplerFloat | Self::SamplerVec2 | Self::SamplerVec3 | Self::SamplerVec4 => {
				return false;
			},
		};
		default
			.as_array()
			.is_some_and(|values| values.len() == components && values.iter().all(gpu_float))
	}

	fn accepts(self, argument: &Parameter) -> bool {
		let finite = |bits: &[u32]| bits.iter().all(|bits| f32::from_bits(*bits).is_finite());
		match (self, argument) {
			(Self::Float, Parameter::Float(value)) => finite(&[*value]),
			(Self::Vec2, Parameter::Vec2(values)) => finite(values),
			(Self::Vec3, Parameter::Vec3(values)) => finite(values),
			(Self::Vec4, Parameter::Vec4(values)) => finite(values),
			(Self::SamplerFloat, Parameter::SamplerFloat(_)) => true,
			(Self::SamplerVec2, Parameter::SamplerVec2(image)) |
			(Self::SamplerVec3, Parameter::SamplerVec3(image)) |
			(Self::SamplerVec4, Parameter::SamplerVec4(image)) => *image != ImageRef::Noise,
			_ => false,
		}
	}
}

/// A material that passed every format rule.
#[derive(Debug)]
pub struct Material {
	params: Vec<ParamKind>,
}

impl Material {
	/// Whether `args` fill the parameters: one each, in order, of the declared
	/// type, with finite numbers, and noise only where one channel is read.
	pub fn arguments_match(&self, args: &[Parameter]) -> bool {
		self.params.len() == args.len() &&
			self.params.iter().zip(args).all(|(kind, argument)| kind.accepts(argument))
	}
}

/// Reads `bytes` as a format version 1 material, checking in the reference's
/// order, so a text with two faults gets the reference's code.
pub fn read_material(bytes: &[u8]) -> Result<Material, FormatError> {
	if bytes.len() > MAX_MATERIAL_BYTES {
		return Err(FormatError::InvalidEncoding);
	}
	let text = core::str::from_utf8(bytes).map_err(|_| FormatError::InvalidEncoding)?;
	if text.starts_with('\u{feff}') || text.contains('\0') {
		return Err(FormatError::InvalidEncoding);
	}
	let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
	let lines: Vec<&str> = normalized.split('\n').collect();
	if lines[0] != HEADER {
		return Err(FormatError::UnsupportedVersion);
	}
	// A required feature may change what every later line means.
	if let Some(features) = lines.get(1).and_then(|line| line.strip_prefix(REQUIRES)) {
		return Err(requires_error(features));
	}
	let mut controls = Vec::new();
	while let Some(json) = lines.get(controls.len() + 1).and_then(|line| line.strip_prefix(CONTROL))
	{
		let Ok(Value::Object(control)) = serde_json::from_str(json) else {
			return Err(FormatError::InvalidParameter);
		};
		if control.keys().any(|key| !PARAM_KEYS.contains(&key.as_str())) {
			return Err(FormatError::InvalidParameter);
		}
		controls.push(control);
	}
	let body = lines[controls.len() + 1..].join("\n");
	if body.split('\n').any(is_directive) {
		return Err(FormatError::InvalidSource);
	}
	let params = read_params(&controls)?;
	if !(1..=MAX_BODY_UNITS).contains(&utf16_len(&body)) || !follows_body_rules(&body) {
		return Err(FormatError::InvalidSource);
	}
	let canonical_body = format!("{}\n", body.trim_end_matches(is_js_space));
	if utf16_len(&canonical_body) > MAX_BODY_UNITS {
		return Err(FormatError::InvalidSource);
	}
	let canonical: String = core::iter::once(HEADER.to_string())
		.chain(controls.iter().map(|control| format!("{CONTROL}{}", control_json(control))))
		.chain([canonical_body])
		.collect::<Vec<_>>()
		.join("\n");
	if canonical.len() > MAX_MATERIAL_BYTES {
		return Err(FormatError::InvalidEncoding);
	}
	if canonical != text {
		return Err(FormatError::NonCanonical);
	}
	Ok(Material { params })
}

/// Version 1 reads no feature, so any `@requires` line is refused: a
/// well-formed one as unsupported.
fn requires_error(features: &str) -> FormatError {
	let Ok(Value::Array(names)) = serde_json::from_str(features) else {
		return FormatError::InvalidSource;
	};
	let names: Option<Vec<&str>> = names
		.iter()
		.map(|name| name.as_str().filter(|name| is_feature_name(name)))
		.collect();
	let Some(names) = names else {
		return FormatError::InvalidSource;
	};
	let mut canonical = names.clone();
	canonical.sort_unstable();
	canonical.dedup();
	if names.is_empty() || serde_json::to_string(&canonical).ok().as_deref() != Some(features) {
		return FormatError::NonCanonical;
	}
	FormatError::UnsupportedFeature
}

/// `^[a-z][a-z0-9]*(?:-[a-z0-9]+)*$`
fn is_feature_name(name: &str) -> bool {
	name.starts_with(|character: char| character.is_ascii_lowercase()) &&
		name.split('-').all(|part| {
			!part.is_empty() &&
				part.chars().all(|character| {
					character.is_ascii_lowercase() || character.is_ascii_digit()
				})
		})
}

/// A line whose first non-blank characters are `//`, optional blanks and `@`.
fn is_directive(line: &str) -> bool {
	line.trim_start_matches(is_js_space)
		.strip_prefix("//")
		.is_some_and(|rest| rest.trim_start_matches(is_js_space).starts_with('@'))
}

fn read_params(controls: &[Map<String, Value>]) -> Result<Vec<ParamKind>, FormatError> {
	if controls.len() > MAX_PARAMS {
		return Err(FormatError::InvalidParameter);
	}
	let mut names = Vec::with_capacity(controls.len());
	let mut params = Vec::with_capacity(controls.len());
	for control in controls {
		let name = control.get("name").and_then(Value::as_str).filter(|name| is_identifier(name));
		let kind = control.get("type").and_then(Value::as_str).and_then(ParamKind::parse);
		let (Some(name), Some(kind)) = (name, kind) else {
			return Err(FormatError::InvalidParameter);
		};
		if names.contains(&name) ||
			!range_allowed(control, kind) ||
			!default_allowed(control, kind) ||
			!presentation_allowed(control, kind)
		{
			return Err(FormatError::InvalidParameter);
		}
		names.push(name);
		params.push(kind);
	}
	Ok(params)
}

/// `^[a-zA-Z][a-zA-Z0-9_]{0,47}$`
fn is_identifier(name: &str) -> bool {
	name.len() <= 48 &&
		name.starts_with(|character: char| character.is_ascii_alphabetic()) &&
		name.chars()
			.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// `min` and `max` come together, on a `float` only, finite and increasing.
fn range_allowed(control: &Map<String, Value>, kind: ParamKind) -> bool {
	if !control.contains_key("min") && !control.contains_key("max") {
		return true;
	}
	let range = finite(control.get("min")).zip(finite(control.get("max")));
	kind == ParamKind::Float && range.is_some_and(|(min, max)| min < max)
}

/// Samplers take no default, not even `null`. Numbers must also be finite as
/// the GPU's 32-bit floats.
fn default_allowed(control: &Map<String, Value>, kind: ParamKind) -> bool {
	match control.get("default") {
		None => true,
		Some(_) if kind.is_sampler() => false,
		Some(default) => kind.accepts_default(default),
	}
}

/// Editor hints: version 1, known keys, short non-blank texts, and a positive
/// `step` on a `float` only.
fn presentation_allowed(control: &Map<String, Value>, kind: ParamKind) -> bool {
	let Some(presentation) = control.get("presentation") else {
		return true;
	};
	let Value::Object(presentation) = presentation else {
		return false;
	};
	let texts_allowed = [("group", 80), ("help", 400), ("unit", 24)].iter().all(|(key, max)| {
		presentation.get(*key).is_none_or(|value| {
			value.as_str().is_some_and(|text| {
				!text.trim_matches(is_js_space).is_empty() && utf16_len(text) <= *max
			})
		})
	});
	let step_allowed = !presentation.contains_key("step") ||
		(kind == ParamKind::Float &&
			finite(presentation.get("step")).is_some_and(|step| step > 0.0));
	presentation.get("version").and_then(Value::as_f64) == Some(1.0) &&
		presentation.keys().all(|key| PRESENTATION_KEYS.contains(&key.as_str())) &&
		texts_allowed &&
		step_allowed
}

fn finite(value: Option<&Value>) -> Option<f64> {
	value.and_then(Value::as_f64).filter(|value| value.is_finite())
}

fn gpu_float(value: &Value) -> bool {
	finite(Some(value)).is_some_and(|value| (value as f32).is_finite())
}

/// The parameter line's JSON as `JSON.stringify` writes it, keys in the
/// format's order.
fn control_json(control: &Map<String, Value>) -> String {
	let fields: Vec<String> = PARAM_KEYS
		.iter()
		.filter_map(|key| {
			let value = match (*key, control.get(*key)?) {
				("presentation", Value::Object(presentation)) => {
					object_json(presentation, &PRESENTATION_KEYS)
				},
				(_, value) => value_json(value),
			};
			Some(format!("\"{key}\":{value}"))
		})
		.collect();
	format!("{{{}}}", fields.join(","))
}

fn object_json(object: &Map<String, Value>, keys: &[&str]) -> String {
	let fields: Vec<String> = keys
		.iter()
		.filter_map(|key| Some(format!("\"{key}\":{}", value_json(object.get(*key)?))))
		.collect();
	format!("{{{}}}", fields.join(","))
}

/// Other objects keep serde's sorted key order, which differs from
/// JavaScript's only for an object `label`, a value no real material uses.
fn value_json(value: &Value) -> String {
	match value {
		Value::Number(number) => number.as_f64().map_or_else(|| number.to_string(), js_number),
		Value::Array(values) => {
			format!("[{}]", values.iter().map(value_json).collect::<Vec<_>>().join(","))
		},
		Value::Object(object) => {
			let fields: Vec<String> = object
				.iter()
				.map(|(key, value)| format!("{}:{}", Value::String(key.clone()), value_json(value)))
				.collect();
			format!("{{{}}}", fields.join(","))
		},
		Value::Null | Value::Bool(_) | Value::String(_) => value.to_string(),
	}
}

/// ECMAScript `Number::toString` for a finite number.
fn js_number(value: f64) -> String {
	if value == 0.0 {
		return "0".to_string();
	}
	if value < 0.0 {
		return format!("-{}", js_number(-value));
	}
	// `{:e}` gives the shortest digits that read back as `value`, as ECMAScript requires.
	let scientific = format!("{value:e}");
	let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
	let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
	let digit_count = digits.len() as i32;
	let point = exponent.parse::<i32>().unwrap_or(0) + 1;
	if digit_count <= point && point <= 21 {
		format!("{digits}{}", "0".repeat((point - digit_count) as usize))
	} else if 0 < point && point <= 21 {
		format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
	} else if -6 < point && point <= 0 {
		format!("0.{}{digits}", "0".repeat(point.unsigned_abs() as usize))
	} else {
		let sign = if point > 0 { '+' } else { '-' };
		let fraction = if digit_count == 1 { String::new() } else { format!(".{}", &digits[1..]) };
		format!("{}{fraction}e{sign}{}", &digits[..1], (point - 1).abs())
	}
}

/// ECMAScript's `\s`, which also decides what `trim` removes.
fn is_js_space(character: char) -> bool {
	matches!(
		character,
		'\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..=
			'\u{200a}' |
				'\u{2028}' | '\u{2029}' |
				'\u{202f}' | '\u{205f}' |
				'\u{3000}' | '\u{feff}'
	)
}

fn utf16_len(text: &str) -> usize {
	text.encode_utf16().count()
}

#[cfg(test)]
mod tests;
