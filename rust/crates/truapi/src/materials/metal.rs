//! Metal shaders for a material: its GLSL ES shaders adapted for naga's GLSL
//! 4.50 reader, as Shaderlab's `prepareForWebGPU` adapts them, then written
//! by naga as Metal's shading language.

use std::{collections::BTreeMap, sync::OnceLock};

use naga::{
	ResourceBinding, ShaderStage,
	back::msl,
	front::glsl,
	valid::{Capabilities, ValidationFlags, Validator},
};
use truapi::latest::MaterialFailure;

use super::{
	glsl::{
		Edit, SAMPLING_FUNCTIONS, Token, apply, bare_name, calls, functions, located_tokens,
		split_arguments, token_end,
	},
	shader::{FRAGMENT_MAIN, Stage, Texture, Uniform, VERTEX_MAIN},
};

/// The vertex function an app looks up in the Metal library.
pub const VERTEX_FUNCTION: &str = "material_vertex";

/// The fragment function an app looks up in the Metal library.
pub const FRAGMENT_FUNCTION: &str = "material_fragment";

/// The fragment stage's resources: the uniform block at binding 0, then a
/// texture and a sampler per slot.
const FRAGMENT_SET: u32 = 0;

/// The vertex stage's resources: its uniform block.
const VERTEX_SET: u32 = 1;

/// The fragment block's Metal buffer. Vertex buffer 0 carries `position`, so
/// the vertex block takes the next.
const FRAGMENT_BUFFER: u8 = 0;
const VERTEX_BUFFER: u8 = 1;

/// Metal's vertex and fragment shaders for `code`, the material's functions
/// after the typed and fallback passes, fed by `uniforms` and `textures`.
pub fn metal_shaders(
	code: &str,
	uniforms: &[Uniform],
	textures: &[Texture],
) -> Result<(String, String), MaterialFailure> {
	// The vertex stage is the same for every material.
	static VERTEX: OnceLock<Result<String, MaterialFailure>> = OnceLock::new();
	let vertex = VERTEX.get_or_init(|| {
		let source = format!(
			"#version 450\nlayout(location = 0) in vec2 position;\nlayout(location = 0) out vec2 uv;\n{}\n{VERTEX_MAIN}\n",
			uniform_block(uniforms, Stage::Vertex)
		);
		let resources = BTreeMap::from([(
			ResourceBinding { group: VERTEX_SET, binding: 0 },
			msl::BindTarget { buffer: Some(VERTEX_BUFFER), ..Default::default() },
		)]);
		translate(&source, ShaderStage::Vertex, VERTEX_FUNCTION, resources)
	});
	let resources: String = textures
		.iter()
		.enumerate()
		.map(|(slot, texture)| {
			let (texture_binding, sampler_binding) = bindings(slot);
			format!(
				"layout(set = {FRAGMENT_SET}, binding = {texture_binding}) uniform texture2D {name};\nlayout(set = {FRAGMENT_SET}, binding = {sampler_binding}) uniform sampler {sampler};\n",
				name = texture.name,
				sampler = sampler_name(&texture.name)
			)
		})
		.collect();
	let fragment = format!(
		"#version 450\nprecision highp float;\nlayout(location = 0) in vec2 uv;\nlayout(location = 0) out vec4 result;\n{}\n{resources}{}\n{FRAGMENT_MAIN}\n",
		uniform_block(uniforms, Stage::Fragment),
		adapt_for_naga(code, textures).ok_or(MaterialFailure::Unsupported)?
	);
	let mut fragment_resources = BTreeMap::from([(
		ResourceBinding { group: FRAGMENT_SET, binding: 0 },
		msl::BindTarget { buffer: Some(FRAGMENT_BUFFER), ..Default::default() },
	)]);
	for slot in 0..textures.len() {
		let index = u8::try_from(slot).map_err(|_| MaterialFailure::Unsupported)?;
		let (texture_binding, sampler_binding) = bindings(slot);
		fragment_resources.insert(
			ResourceBinding { group: FRAGMENT_SET, binding: texture_binding },
			msl::BindTarget { texture: Some(index), ..Default::default() },
		);
		fragment_resources.insert(
			ResourceBinding { group: FRAGMENT_SET, binding: sampler_binding },
			msl::BindTarget {
				sampler: Some(msl::BindSamplerTarget::Resource(index)),
				..Default::default()
			},
		);
	}
	Ok((
		vertex.clone()?,
		translate(&fragment, ShaderStage::Fragment, FRAGMENT_FUNCTION, fragment_resources)?,
	))
}

/// Whether naga's GLSL reader takes `word` as a type: GLSL ES leaves these
/// names free.
fn naga_type_name(word: &str) -> bool {
	const DIMENSIONS: [&str; 9] =
		["1D", "1DArray", "2D", "2DArray", "2DMS", "2DMSArray", "3D", "Cube", "CubeArray"];
	let texture = word.strip_prefix(['i', 'u']).unwrap_or(word);
	matches!(word, "sampler" | "samplerShadow") ||
		texture
			.strip_prefix("texture")
			.is_some_and(|dimension| DIMENSIONS.contains(&dimension))
}

/// The texture and sampler bindings of texture slot `slot`: binding 0 is the
/// uniform block.
fn bindings(slot: usize) -> (u32, u32) {
	let texture = 2 * slot as u32 + 1;
	(texture, texture + 1)
}

/// The sampler split off a sampler named `name`. Material names cannot start
/// with `_shaderlab_`, so it never meets one of them.
fn sampler_name(name: &str) -> String {
	format!("_shaderlab_sampler_{name}")
}

/// The stage's non-opaque uniforms as one std140 block, in the description's
/// order, which is where naga places them.
fn uniform_block(uniforms: &[Uniform], stage: Stage) -> String {
	let (set, name) = match stage {
		Stage::Vertex => (VERTEX_SET, "_shaderlab_vertex_uniforms"),
		Stage::Fragment => (FRAGMENT_SET, "_shaderlab_fragment_uniforms"),
	};
	let members: String = uniforms
		.iter()
		.filter(|uniform| uniform.stage == stage)
		.map(|uniform| format!("  {} {};\n", uniform.kind.glsl(), uniform.name))
		.collect();
	format!("layout(std140, set = {set}, binding = 0) uniform {name} {{\n{members}}};")
}

/// Adapts `code` for naga. naga has no sampler that is both texture and
/// sampler: a `sampler2D` parameter becomes a texture and a sampler, a sampler
/// handed to a sampling function is rebuilt from the two, and one handed to a
/// function carries its sampler beside it. Precision qualifiers mean nothing in
/// GLSL 4.50 and naga refuses some of them, so they go. Names naga reads as
/// types are renamed. naga loses a
/// declaration whose parameters have no names, so each gets one.
fn adapt_for_naga(code: &str, textures: &[Texture]) -> Option<String> {
	let tokens = located_tokens(code)?;
	let mut edits: Vec<Edit> = tokens
		.iter()
		.filter_map(|located| {
			let Token::Word(word) = located.1 else { return None };
			let text = match word {
				"lowp" | "mediump" | "highp" => String::new(),
				word if naga_type_name(word) => format!("_shaderlab_name_{word}"),
				_ => return None,
			};
			Some(Edit { start: located.0, end: token_end(located), text })
		})
		.collect();
	for function in functions(&tokens)? {
		let mut samplers: Vec<&str> =
			textures.iter().map(|texture| texture.name.as_str()).collect();
		for (index, param) in function.params.iter().enumerate() {
			let unnamed = format!("_shaderlab_parameter_{index}");
			let name = param.name.unwrap_or(&unnamed);
			let mut text = if param.name.is_none() { format!(" {name}") } else { String::new() };
			if param.kind == "sampler2D" {
				edits.push(Edit {
					start: param.kind_at,
					end: param.kind_at + param.kind.len(),
					text: "texture2D".to_string(),
				});
				text.push_str(&format!(", sampler {}", sampler_name(name)));
				samplers.extend(param.name);
			}
			edits.push(Edit { start: param.end, end: param.end, text });
		}
		let Some(body) = function.body else { continue };
		for (name, close) in calls(&tokens, body)? {
			let Token::Word(function_name) = tokens[name].1 else { continue };
			let built_in =
				SAMPLING_FUNCTIONS.contains(&function_name) || function_name == "textureSize";
			for argument in split_arguments(&tokens[name + 2..close]) {
				let Some((at, sampler)) =
					bare_name(argument).filter(|(_, name)| samplers.contains(name))
				else {
					continue;
				};
				if built_in {
					edits.push(Edit {
						start: at,
						end: at + sampler.len(),
						text: format!("sampler2D({sampler}, {})", sampler_name(sampler)),
					});
				} else {
					let end = token_end(argument.last()?);
					edits.push(Edit {
						start: end,
						end,
						text: format!(", {}", sampler_name(sampler)),
					});
				}
			}
		}
	}
	Some(apply(code, edits))
}

/// Reads `source` with naga, checks it, and writes it as Metal, the entry point
/// named `function` and its resources at `resources`.
fn translate(
	source: &str,
	stage: ShaderStage,
	function: &str,
	resources: msl::BindingMap,
) -> Result<String, MaterialFailure> {
	let mut module = glsl::Frontend::default()
		.parse(&glsl::Options::from(stage), source)
		.map_err(|error| unsupported("read", &error))?;
	for entry_point in &mut module.entry_points {
		entry_point.name = function.to_string();
	}
	let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
		.validate(&module)
		.map_err(|error| unsupported("validate", &error))?;
	let options = msl::Options {
		per_entry_point_map: BTreeMap::from([(
			function.to_string(),
			msl::EntryPointResources { resources, ..Default::default() },
		)]),
		..Default::default()
	};
	let (metal, _) = msl::write_string(&module, &info, &options, &msl::PipelineOptions::default())
		.map_err(|error| unsupported("write", &error))?;
	Ok(metal)
}

fn unsupported(step: &str, error: &dyn core::fmt::Debug) -> MaterialFailure {
	tracing::debug!(step, ?error, "naga could not translate a material");
	MaterialFailure::Unsupported
}

#[cfg(test)]
mod tests;
