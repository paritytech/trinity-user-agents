use serde::Deserialize;
use truapi::latest::MaterialFailure;

use super::*;
use crate::materials::format::{Material, read_material};

/// What Shaderlab's own compile step writes, from `scripts/shaderlab-shader-fixtures.ts` run on
/// shaderlab-spa `main` at `754f78d0`.
const COMPILED: &str = include_str!("../../../tests/fixtures/materials/compiled.v9.json");

/// Shaderlab's uniform layout vectors, from shaderlab-spa PR #90 at `d5f5148d`.
const LAYOUT: &str = include_str!("../../../tests/fixtures/materials/webgpu-layout.v1.json");

#[derive(Deserialize)]
struct Compiled {
	cases: Vec<CompiledCase>,
}

#[derive(Deserialize)]
struct CompiledCase {
	name: String,
	text: String,
	normal: Stages,
	fallback: Stages,
}

#[derive(Deserialize)]
struct Stages {
	vertex: String,
	fragment: String,
}

/// Shaderlab's parser places its edits after the spaces and comments that
/// follow a node, which changes nothing a shader does, so shaders are compared
/// without them.
fn without_spacing(source: &str) -> String {
	let mut kept = String::new();
	let mut rest = source;
	while let Some(next) = rest.chars().next() {
		if let Some(comment) = rest.strip_prefix("//") {
			rest = comment.find('\n').map_or("", |end| &comment[end..]);
		} else if let Some(comment) = rest.strip_prefix("/*") {
			rest = comment.find("*/").map_or("", |end| &comment[end + 2..]);
		} else {
			if !next.is_whitespace() {
				kept.push(next);
			}
			rest = &rest[next.len_utf8()..];
		}
	}
	kept
}

fn material(text: &str) -> Material {
	read_material(text.as_bytes()).unwrap_or_else(|error| panic!("{error}: {text}"))
}

/// Android compiles this text, so it must be what Shaderlab's browser
/// renderer compiles, or a card draws differently on a phone.
#[test]
fn android_shaders_are_what_shaderlab_compiles() {
	let compiled: Compiled = serde_json::from_str(COMPILED).unwrap();
	for case in compiled.cases {
		let material = material(&case.text);
		for (variant, float_filterable, expected) in
			[("normal", true, &case.normal), ("fallback", false, &case.fallback)]
		{
			let shader = material
				.shader(ShaderLanguage::GlslEs, float_filterable)
				.unwrap_or_else(|failure| panic!("{} {variant}: {failure:?}", case.name));
			assert_eq!(
				without_spacing(&shader.vertex),
				without_spacing(&expected.vertex),
				"{} {variant} vertex",
				case.name
			);
			assert_eq!(
				without_spacing(&shader.fragment),
				without_spacing(&expected.fragment),
				"{} {variant} fragment",
				case.name
			);
		}
	}
}

fn with_height(body: &str) -> String {
	format!(
		"// @material 1\n// @control {{\"name\":\"height\",\"label\":\"Height\",\"type\":\"samplerFloat\"}}\n{body}\n"
	)
}

/// The core finds samplers by name, not by scope, so a name that would hide a
/// sampler is refused rather than rewritten wrongly. No real material uses one.
#[test]
fn a_name_hiding_a_sampler_and_a_sampler_array_are_unsupported() {
	for body in [
		"vec4 material(vec2 uv) {\n  vec2 r_input = uv;\n  return vec4(r_input, 0.0, 1.0);\n}",
		"float spread(vec2 sp_height) {\n  return length(sp_height);\n}\nvec4 material(vec2 uv) {\n  return vec4(spread(uv));\n}",
		"float read(samplerFloat image, vec2 point) {\n  float image = 1.0;\n  return image;\n}\nvec4 material(vec2 uv) {\n  return vec4(read(sp_height, uv));\n}",
		"vec4 pick(sampler2D images[2], vec2 point) {\n  return texture(images[0], point);\n}\nvec4 material(vec2 uv) {\n  return texture(r_input, uv);\n}",
	] {
		for float_filterable in [true, false] {
			assert_eq!(
				material(&with_height(body))
					.shader(ShaderLanguage::GlslEs, float_filterable)
					.err(),
				Some(MaterialFailure::Unsupported),
				"{body}"
			);
		}
	}
}

fn reads(body: &str) -> (bool, bool) {
	let shader = material(&format!("// @material 1\n{body}\n"))
		.shader(ShaderLanguage::GlslEs, true)
		.unwrap();
	(shader.reads_time, shader.reads_motion)
}

/// A node whose layers read neither time nor motion is drawn once, so a
/// material that moves must never be reported as still.
#[test]
fn a_material_reads_time_or_motion_only_when_its_body_does() {
	let read = |input: &str| {
		reads(&format!("vec4 material(vec2 uv) {{\n  return texture(r_input, uv) * {input};\n}}"))
	};

	assert_eq!(read("1.0"), (false, false));
	assert_eq!(read("sl_time"), (true, false));
	assert_eq!(read("sl_tilt.x"), (false, true));
	assert_eq!(read("sl_orientation.y"), (false, true));
	assert_eq!(read("(sl_rotation * vec3(1.0)).z"), (false, true));
}

#[derive(Deserialize)]
struct LayoutVectors {
	cases: Vec<LayoutCase>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayoutCase {
	name: String,
	text: String,
	float_linear_fallback: bool,
	expect: LayoutExpect,
}

#[derive(Deserialize)]
struct LayoutExpect {
	ok: bool,
	vertex: Option<Block>,
	fragment: Option<Block>,
	textures: Option<Vec<TextureExpect>>,
}

#[derive(Deserialize, Debug, PartialEq)]
struct Block {
	size: u32,
	members: Vec<Member>,
}

#[derive(Deserialize, Debug, PartialEq)]
struct Member {
	name: String,
	#[serde(rename = "type")]
	kind: String,
	offset: u32,
	role: String,
}

#[derive(Deserialize)]
struct TextureExpect {
	name: String,
	role: String,
}

fn block(shader: &MaterialShader, stage: Stage) -> Block {
	let size = match stage {
		Stage::Vertex => shader.vertex_block_size,
		Stage::Fragment => shader.fragment_block_size,
	};
	let members = shader
		.uniforms
		.iter()
		.filter(|uniform| uniform.stage == stage)
		.map(|uniform| Member {
			name: uniform.name.clone(),
			kind: uniform.kind.glsl().to_string(),
			offset: uniform.offset,
			role: match uniform.role {
				UniformRole::Time |
				UniformRole::Tilt |
				UniformRole::Resolution |
				UniformRole::Orientation |
				UniformRole::Rotation => "environment",
				UniformRole::Argument(_) => "argument",
				UniformRole::SamplingMode(_) => "sampling-mode",
				UniformRole::TargetFlip => "target-flip",
			}
			.to_string(),
		})
		.collect();
	Block { size, members }
}

/// The offsets are where the iOS app writes each value, so they must be
/// Shaderlab's. One vector asks for the normal variant on a GPU that cannot
/// filter float images, which this API never builds.
#[test]
fn the_uniform_layout_matches_the_vectors() {
	let vectors: LayoutVectors = serde_json::from_str(LAYOUT).unwrap();
	for case in vectors.cases.into_iter().filter(|case| case.expect.ok) {
		let shader = material(&case.text)
			.shader(ShaderLanguage::GlslEs, !case.float_linear_fallback)
			.unwrap();

		assert_eq!(Some(block(&shader, Stage::Vertex)), case.expect.vertex, "{}", case.name);
		assert_eq!(Some(block(&shader, Stage::Fragment)), case.expect.fragment, "{}", case.name);
		let textures: Vec<_> = shader
			.textures
			.iter()
			.map(|texture| {
				let role = match texture.role {
					TextureRole::Base => "base",
					TextureRole::Input => "input",
					TextureRole::Noise => "noise",
					TextureRole::Argument(_) => "argument",
				};
				(texture.name.clone(), role.to_string())
			})
			.collect();
		let expected: Vec<_> = case
			.expect
			.textures
			.unwrap()
			.into_iter()
			.map(|texture| (texture.name, texture.role))
			.collect();
		assert_eq!(textures, expected, "{}", case.name);
	}
}

/// An app fills each uniform and texture slot from its role: argument `n` is
/// the layer's `n`th argument, counting samplers, and a sampling mode names its
/// texture slot.
#[test]
fn every_uniform_and_texture_says_what_fills_it() {
	let text = "// @material 1\n// @control {\"name\":\"tint\",\"label\":\"Tint\",\"type\":\"vec3\",\"default\":[1,0.5,0.25]}\n// @control {\"name\":\"height\",\"label\":\"Height map\",\"type\":\"samplerFloat\"}\nvec4 material(vec2 uv) {\n  float h = texture(sp_height, uv);\n  return vec4(texture(r_input, uv).rgb * p_tint * h, 1.0);\n}\n";
	let shader = material(text).shader(ShaderLanguage::GlslEs, false).unwrap();

	let uniform = |name: &str, kind, stage, offset, role| Uniform {
		name: name.to_string(),
		kind,
		stage,
		offset,
		role,
	};
	assert_eq!(
		shader.uniforms,
		[
			uniform(
				"_shaderlab_target_flip",
				UniformKind::Float,
				Stage::Vertex,
				0,
				UniformRole::TargetFlip
			),
			uniform("sl_time", UniformKind::Float, Stage::Fragment, 0, UniformRole::Time),
			uniform("sl_tilt", UniformKind::Vec2, Stage::Fragment, 8, UniformRole::Tilt),
			uniform(
				"sl_resolution",
				UniformKind::Vec2,
				Stage::Fragment,
				16,
				UniformRole::Resolution
			),
			uniform(
				"sl_orientation",
				UniformKind::Vec3,
				Stage::Fragment,
				32,
				UniformRole::Orientation
			),
			uniform("sl_rotation", UniformKind::Mat3, Stage::Fragment, 48, UniformRole::Rotation),
			uniform("p_tint", UniformKind::Vec3, Stage::Fragment, 96, UniformRole::Argument(0)),
			uniform(
				"_shaderlab_sampling_r_input",
				UniformKind::Int,
				Stage::Fragment,
				108,
				UniformRole::SamplingMode(1)
			),
			uniform(
				"_shaderlab_sampling_sp_height",
				UniformKind::Int,
				Stage::Fragment,
				112,
				UniformRole::SamplingMode(3)
			),
		]
	);
	let texture = |name: &str, role| Texture { name: name.to_string(), role };
	assert_eq!(
		shader.textures,
		[
			texture("r_base", TextureRole::Base),
			texture("r_input", TextureRole::Input),
			texture("r_noise", TextureRole::Noise),
			texture("sp_height", TextureRole::Argument(1)),
		]
	);
	assert_eq!((shader.vertex_block_size, shader.fragment_block_size), (16, 128));
}
