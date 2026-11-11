use naga::{ResourceBinding, TypeInner};
use serde::Deserialize;
use truapi::latest::MaterialFailure;

use super::*;
use crate::materials::{
	format::read_material,
	shader::{ShaderLanguage, Stage, Uniform},
};

/// The materials the Android shaders are checked with, from
/// `scripts/shaderlab-shader-fixtures.ts` on shaderlab-spa `main` at `754f78d0`.
const COMPILED: &str = include_str!("../../../tests/fixtures/materials/compiled.v9.json");

#[derive(Deserialize)]
struct Compiled {
	cases: Vec<CompiledCase>,
}

#[derive(Deserialize)]
struct CompiledCase {
	name: String,
	text: String,
}

/// Each `name [[kind(index)]]` in `msl`.
fn bound(msl: &str, kind: &str) -> Vec<(String, usize)> {
	let marker = format!(" [[{kind}(");
	msl.match_indices(&marker)
		.map(|(at, _)| {
			let name = msl[..at]
				.rsplit(|character: char| !character.is_ascii_alphanumeric() && character != '_');
			let index = msl[at + marker.len()..].split(')').next().unwrap();
			(name.into_iter().next().unwrap().to_string(), index.parse().unwrap())
		})
		.collect()
}

/// The iOS app binds each image and each value where the description says,
/// so naga's Metal output must use exactly those slots.
#[test]
fn every_material_translates_with_its_slots_where_the_description_says() {
	let compiled: Compiled = serde_json::from_str(COMPILED).unwrap();
	for case in compiled.cases {
		let material = read_material(case.text.as_bytes()).unwrap();
		for float_filterable in [true, false] {
			let shader = material
				.shader(ShaderLanguage::Metal, float_filterable)
				.unwrap_or_else(|failure| panic!("{} {float_filterable}: {failure:?}", case.name));

			assert!(shader.vertex.contains(&format!(" {VERTEX_FUNCTION}(")), "{}", case.name);
			assert!(shader.fragment.contains(&format!(" {FRAGMENT_FUNCTION}(")), "{}", case.name);
			let slots = |msl: &str, kind| {
				bound(msl, kind).into_iter().map(|(_, slot)| slot).collect::<Vec<_>>()
			};
			assert_eq!(slots(&shader.vertex, "buffer"), [1], "{}", case.name);
			assert_eq!(slots(&shader.fragment, "buffer"), [0], "{}", case.name);
			let textures = bound(&shader.fragment, "texture");
			if case.text.contains("r_input") {
				assert!(textures.contains(&("r_input".to_string(), 1)), "{}", case.name);
			}
			for (name, slot) in textures {
				assert_eq!(shader.textures[slot].name, name, "{}", case.name);
			}
			for (name, slot) in bound(&shader.fragment, "sampler") {
				assert_eq!(sampler_name(&shader.textures[slot].name), name, "{}", case.name);
			}
		}
	}
}

/// naga lays out the uniform block itself, and the app writes at the offsets
/// in the description, so the two must agree member for member. The
/// description's size is rounded up to 16 bytes as std140 asks, and naga may
/// count less, so a buffer of the description's size always holds the block.
#[test]
fn naga_lays_out_the_uniform_blocks_as_the_description_does() {
	let compiled: Compiled = serde_json::from_str(COMPILED).unwrap();
	for case in compiled.cases {
		let material = read_material(case.text.as_bytes()).unwrap();
		for float_filterable in [true, false] {
			let shader = material.shader(ShaderLanguage::GlslEs, float_filterable).unwrap();
			for (stage, set, size) in [
				(Stage::Vertex, VERTEX_SET, shader.vertex_block_size),
				(Stage::Fragment, FRAGMENT_SET, shader.fragment_block_size),
			] {
				let source = format!(
					"#version 450\n{}\nvoid main() {{}}\n",
					uniform_block(&shader.uniforms, stage)
				);
				let naga_stage = match stage {
					Stage::Vertex => naga::ShaderStage::Vertex,
					Stage::Fragment => naga::ShaderStage::Fragment,
				};
				let module = naga::front::glsl::Frontend::default()
					.parse(&naga::front::glsl::Options::from(naga_stage), &source)
					.unwrap();
				let block = module
					.global_variables
					.iter()
					.find(|(_, global)| {
						global.binding == Some(ResourceBinding { group: set, binding: 0 })
					})
					.map(|(_, global)| &module.types[global.ty])
					.unwrap();
				let TypeInner::Struct { members, span } = &block.inner else {
					panic!("not a block")
				};
				let laid_out: Vec<_> = members
					.iter()
					.map(|member| (member.name.clone().unwrap(), member.offset))
					.collect();
				let described: Vec<_> = shader
					.uniforms
					.iter()
					.filter(|uniform: &&Uniform| uniform.stage == stage)
					.map(|uniform| (uniform.name.clone(), uniform.offset))
					.collect();
				assert_eq!(laid_out, described, "{} {stage:?}", case.name);
				assert!(*span <= size, "{} {stage:?}: naga {span}, described {size}", case.name);
			}
		}
	}
}

/// A material that keeps the format's rules can still break GLSL's, and the
/// device could not compile it: the node draws plain.
#[test]
fn a_body_that_does_not_type_check_is_unsupported() {
	let text = "// @material 1\nvec4 material(vec2 uv) {\n  return 1;\n}\n";

	assert_eq!(
		read_material(text.as_bytes())
			.unwrap()
			.shader(ShaderLanguage::Metal, true)
			.err(),
		Some(MaterialFailure::Unsupported)
	);
}

/// A split-off sampler's name can never meet a material's own names, so a
/// material draws on iOS wherever it draws on Android.
#[test]
fn split_samplers_never_collide_with_a_material_name() {
	let text = "// @material 1\n// @control {\"name\":\"foo\",\"label\":\"Foo\",\"type\":\"samplerFloat\"}\n// @control {\"name\":\"foo_sampler\",\"label\":\"Foo sampler\",\"type\":\"samplerFloat\"}\nvec4 material(vec2 uv) {\n  float foo_sampler = texture(sp_foo, uv);\n  return vec4(foo_sampler + texture(sp_foo_sampler, uv));\n}\n";
	let material = read_material(text.as_bytes()).unwrap();

	for float_filterable in [true, false] {
		assert!(
			material.shader(ShaderLanguage::Metal, float_filterable).is_ok(),
			"{float_filterable}"
		);
	}
}

fn translates(text: &str) -> bool {
	let material = read_material(text.as_bytes()).unwrap_or_else(|error| panic!("{error}: {text}"));
	[true, false]
		.iter()
		.all(|float_filterable| material.shader(ShaderLanguage::Metal, *float_filterable).is_ok())
}

/// The compiler joins a backslash-ended line to the next, so a sampler named
/// across two lines is still read as one, on iOS as on Android.
#[test]
fn a_sampler_named_across_a_line_continuation_translates() {
	assert!(translates(
		"// @material 1\n// @control {\"name\":\"height\",\"label\":\"Height\",\"type\":\"samplerFloat\"}\nvec4 material(vec2 uv) {\n  return vec4(texture(s\\\np_height, uv));\n}\n"
	));
}

/// `sampler` and `texture2D` are ordinary names in GLSL ES but types in the
/// GLSL naga reads, so a material using them still draws on iOS.
#[test]
fn names_only_naga_reserves_translate() {
	assert!(translates(
		"// @material 1\nvec4 material(vec2 uv) {\n  float sampler = uv.x;\n  float texture2D = uv.y;\n  return vec4(sampler, texture2D, 0.0, 1.0);\n}\n"
	));
}

/// Apple's compiler runs only on macOS, so CI writes every Metal shader the
/// core builds into `TRUAPI_METAL_OUT` for a macOS job to compile with it.
/// Without the variable, nothing is written.
#[test]
fn every_metal_shader_is_written_for_apples_compiler() {
	let Some(directory) = std::env::var_os("TRUAPI_METAL_OUT") else {
		return;
	};
	std::fs::create_dir_all(&directory).unwrap();
	let compiled: Compiled = serde_json::from_str(COMPILED).unwrap();
	for (index, case) in compiled.cases.iter().enumerate() {
		let material = read_material(case.text.as_bytes()).unwrap();
		for float_filterable in [true, false] {
			let shader = material.shader(ShaderLanguage::Metal, float_filterable).unwrap();
			for (stage, source) in [("vertex", &shader.vertex), ("fragment", &shader.fragment)] {
				let name = format!("{index}-{float_filterable}-{stage}.metal");
				std::fs::write(std::path::Path::new(&directory).join(name), source).unwrap();
			}
		}
	}
}
