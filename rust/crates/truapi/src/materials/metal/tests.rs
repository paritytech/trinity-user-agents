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
				assert_eq!(
					format!("{}_sampler", shader.textures[slot].name),
					name,
					"{}",
					case.name
				);
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
