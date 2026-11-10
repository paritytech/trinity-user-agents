use serde::Deserialize;
use truapi::latest::{ImageRef, ImageSource, MaterialFailure, Parameter};

use super::*;
use crate::host_internal::bulletin::{preimage_cid, preimage_key};

/// Shaderlab's format vectors, from shaderlab-spa PR #97 at `4fb610d1`.
const VECTORS: &str = include_str!("../../../tests/fixtures/materials/material-format.v1.json");

#[derive(Deserialize)]
struct Vectors {
	cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
	name: String,
	text: Option<String>,
	bytes_hex: Option<String>,
	expect: Expect,
}

#[derive(Deserialize)]
struct Expect {
	id: Option<String>,
	code: Option<String>,
}

/// The format's vectors are its test: a host that disagrees with one reads
/// materials differently from every other host.
#[test]
fn every_format_vector_gets_its_verdict_and_code() {
	let vectors: Vectors = serde_json::from_str(VECTORS).unwrap();
	let mut expected = Vec::new();
	let mut actual = Vec::new();
	for case in vectors.cases {
		let bytes = match (case.text, case.bytes_hex) {
			(Some(text), _) => text.into_bytes(),
			(None, Some(hex)) => hex::decode(hex).unwrap(),
			(None, None) => panic!("{} has no text", case.name),
		};
		let verdict = read_material(&bytes)
			.map(|_| preimage_cid(&preimage_key(&bytes)))
			.map_err(|error| error.to_string());
		expected
			.push((case.name.clone(), case.expect.id.ok_or(case.expect.code.unwrap_or_default())));
		actual.push((case.name, verdict));
	}

	assert_eq!(actual, expected);
}

fn material(body: &str) -> Vec<u8> {
	format!("// @material 1\n{body}\n").into_bytes()
}

fn code(text: &[u8]) -> Option<FormatError> {
	read_material(text).err()
}

const ENTRY: &str = "vec4 material(vec2 uv) {\n  return texture(r_input, uv);\n}";

/// Arguments are matched to parameters by position, so the reader must keep
/// the declaration order.
#[test]
fn a_material_gives_its_parameters_in_declaration_order() {
	let text = "// @material 1\n// @control {\"name\":\"tint\",\"label\":\"Tint\",\"type\":\"vec3\",\"default\":[1,0.5,0.25]}\n// @control {\"name\":\"height\",\"label\":\"Height\",\"type\":\"samplerFloat\"}\nvec4 material(vec2 uv) {\n  return texture(r_input, uv);\n}\n";

	assert_eq!(
		read_material(text.as_bytes()).map(|material| material.params),
		Ok(vec![ParamKind::Vec3, ParamKind::SamplerFloat])
	);
}

/// Hosts must refuse what Shaderlab refuses, or one host draws a material
/// another rejects.
#[test]
fn every_body_rule_refuses_with_invalid_source() {
	let mut refused = vec![
		format!("#define K 1.0\n{ENTRY}"),
		"vec4 material(vec2 uv) {\n  return vec4(gl_FragCoord);\n}".to_string(),
		"vec4 material(vec2 uv) {\n  float _shaderlab_x = 1.0;\n  return vec4(_shaderlab_x);\n}"
			.to_string(),
		"vec4 material(vec2 uv) {\n  return vec4(sl_pressure);\n}".to_string(),
		"vec4 material(vec2 uv) {\n  float r_tmp = 1.0;\n  return vec4(r_tmp);\n}".to_string(),
		format!("void main() {{\n}}\n{ENTRY}"),
		format!("vec4 texture(sampler2D image, vec2 point) {{\n  return vec4(0.0);\n}}\n{ENTRY}"),
		"vec3 material(vec2 uv) {\n  return vec3(1.0);\n}".to_string(),
		format!("{ENTRY}\nvec4 material(float x) {{\n  return vec4(x);\n}}"),
		"vec4 shade(vec2 uv) {\n  return vec4(1.0);\n}".to_string(),
		format!("uni\\\nform float k;\n{ENTRY}"),
		format!("/* a *\\\n/ uniform float k; */\n{ENTRY}"),
		format!("// note \\\nuniform float k;\n{ENTRY}"),
	];
	for keyword in ["uniform", "layout", "attribute", "varying", "buffer", "shared", "precision"] {
		refused.push(format!("{keyword} float k;\n{ENTRY}"));
	}

	for body in refused {
		assert_eq!(code(&material(&body)), Some(FormatError::InvalidSource), "{body}");
	}
}

/// The rules refuse only what Shaderlab refuses: qualifiers, forward
/// declarations, loops, helpers, struct names and every renderer input stay
/// allowed, a banned word inside a comment is only a comment, and so is a
/// comment line ending in a backslash.
#[test]
fn the_body_rules_accept_what_shaderlab_accepts() {
	let accepted = [
		"vec4 material(in vec2 uv) {\n  return texture(r_input, uv);\n}".to_string(),
		format!("vec4 material(vec2);\n{ENTRY}"),
		format!("highp float half_of(highp float x) {{\n  return x * 0.5;\n}}\n{ENTRY}"),
		"vec4 material(vec2 uv) {\n  float sum = 0.0;\n  for (int i = 0; i < 4; i++) {\n    sum += 0.25;\n  }\n  return vec4(sum);\n}".to_string(),
		format!("// uniform gl_x main() #define\n/* layout sl_x */\n{ENTRY}"),
		format!("// see below \\\n\n{ENTRY}"),
		format!("struct sl_Light {{\n  float strength;\n}};\n{ENTRY}"),
		"vec4 material(vec2 uv) {\n  vec4 sum = texture(r_input, uv) + texture(r_base, uv) + vec4(texture(r_noise, uv));\n  sum.xy += sl_tilt * sl_time + sl_resolution;\n  sum.xyz += sl_rotation * sl_orientation;\n  return sum;\n}".to_string(),
	];

	for body in accepted {
		assert_eq!(code(&material(&body)), None, "{body}");
	}
}

/// Shaderlab's parser ends with a second pass over the canonical text, so a
/// text with two faults gets the code of the one that pass finds first.
#[test]
fn the_reference_last_pass_finds_long_bodies_and_long_canonical_texts() {
	let header = "// @material 1\n";
	let mut body = format!("{ENTRY}\n//");
	body.push_str(&"x".repeat(MAX_BODY_UNITS - body.len()));
	assert_eq!(
		code(format!("{header}{body}").as_bytes()),
		Some(FormatError::InvalidSource),
		"the body grows past its limit with its final newline"
	);

	let head = "// @material 1\n// @control {\"name\":\"k\",\"label\":\"";
	let tail = format!("\",\"type\":\"float\",\"default\":1e3}}\n{ENTRY}\n");
	let label = "x".repeat(MAX_MATERIAL_BYTES - head.len() - tail.len());
	assert_eq!(
		code(format!("{head}{label}{tail}").as_bytes()),
		Some(FormatError::InvalidEncoding),
		"the canonical text spells 1e3 as 1000, one byte over the limit"
	);
}

fn with_default(default: &str) -> Vec<u8> {
	format!(
		"// @material 1\n// @control {{\"name\":\"k\",\"label\":\"K\",\"type\":\"float\",\"default\":{default}}}\n{ENTRY}\n"
	)
	.into_bytes()
}

/// A number is canonical when it is spelled as JavaScript writes the value
/// JavaScript reads, so the reader must read and write numbers that way.
#[test]
fn numbers_are_read_and_written_as_javascript_does() {
	assert_eq!(code(&with_default("0.9724722717202353")), None, "read to the last digit");
	assert_eq!(
		code(&with_default("9007199254740993")),
		Some(FormatError::NonCanonical),
		"JavaScript reads it as 9007199254740992"
	);
	assert_eq!(code(&with_default("-0")), Some(FormatError::NonCanonical));
	assert_eq!(
		code(&with_default("1e39")),
		Some(FormatError::InvalidParameter),
		"finite as a double, not as a GPU float"
	);
	for (value, spelling) in [
		(0.01, "0.01"),
		(1.5, "1.5"),
		(1e21, "1e+21"),
		(123456789012345680000.0, "123456789012345680000"),
		(0.000001, "0.000001"),
		(1e-7, "1e-7"),
		(-2.5e-8, "-2.5e-8"),
		(0.1 + 0.2, "0.30000000000000004"),
	] {
		assert_eq!(js_number(value), spelling);
	}
}

fn with_group(group: char) -> Vec<u8> {
	format!(
		"// @material 1\n// @control {{\"name\":\"k\",\"label\":\"K\",\"type\":\"float\",\"default\":1,\"presentation\":{{\"version\":1,\"group\":\"{group}\"}}}}\n{ENTRY}\n"
	)
	.into_bytes()
}

/// Blank means blank as JavaScript sees it, which differs from Rust's
/// whitespace in both directions.
#[test]
fn whitespace_is_javascript_whitespace() {
	assert_eq!(code(&with_group('\u{feff}')), Some(FormatError::InvalidParameter));
	assert_eq!(code(&with_group('\u{85}')), None);
}

/// A host that cannot read a material's version or features says so as
/// `Unsupported`, which a product reads differently from a broken material.
#[test]
fn each_code_maps_to_its_failure() {
	let failures = [
		FormatError::InvalidEncoding,
		FormatError::UnsupportedVersion,
		FormatError::UnsupportedFeature,
		FormatError::InvalidParameter,
		FormatError::InvalidSource,
		FormatError::NonCanonical,
	]
	.map(MaterialFailure::from);

	assert_eq!(
		failures,
		[
			MaterialFailure::InvalidMaterial,
			MaterialFailure::Unsupported,
			MaterialFailure::Unsupported,
			MaterialFailure::InvalidMaterial,
			MaterialFailure::InvalidMaterial,
			MaterialFailure::InvalidMaterial,
		]
	);
}

fn tint_and_height() -> Material {
	Material { params: vec![ParamKind::Vec3, ParamKind::SamplerVec2] }
}

fn image() -> ImageRef {
	ImageRef::Source(ImageSource::Archive("height.png".to_string()))
}

/// A layer whose arguments do not fit its material never reaches the GPU,
/// and the noise image has one channel, so it fills only `samplerFloat`.
#[test]
fn arguments_match_by_count_type_and_finite_value() {
	let material = tint_and_height();
	let tint = Parameter::Vec3([1f32.to_bits(); 3]);
	let height = Parameter::SamplerVec2(image());

	assert!(material.arguments_match(&[tint.clone(), height.clone()]));
	for args in [
		vec![tint.clone()],
		vec![tint.clone(), height.clone(), height.clone()],
		vec![Parameter::Vec2([0; 2]), height.clone()],
		vec![Parameter::Vec3([f32::NAN.to_bits(), 0, 0]), height.clone()],
		vec![Parameter::Vec3([0, f32::INFINITY.to_bits(), 0]), height],
		vec![tint, Parameter::SamplerVec2(ImageRef::Noise)],
	] {
		assert!(!material.arguments_match(&args), "{args:?}");
	}
	assert!(
		Material { params: vec![ParamKind::SamplerFloat] }
			.arguments_match(&[Parameter::SamplerFloat(ImageRef::Noise)])
	);
}
