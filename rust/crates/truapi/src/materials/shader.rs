//! The shaders an app compiles to draw a material, and what feeds them.

use truapi::latest::MaterialFailure;

use super::{
	format::{Param, ParamKind},
	glsl::{
		Edit, Function, SAMPLING_FUNCTIONS, Token, apply, bare_name, calls, functions,
		located_tokens, split_arguments, token_end, tokenize,
	},
	metal::metal_shaders,
};

/// The shader language an app compiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum ShaderLanguage {
	/// GLSL ES 3.00, for OpenGL ES 3.0.
	GlslEs,
	/// Metal's shading language.
	Metal,
}

/// A material's shaders and how to feed them.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MaterialShader {
	/// The vertex shader: a full-screen quad from one `vec2 position` attribute
	/// in clip space, drawn without culling. The fragment shader works out
	/// `uv`, which runs from the node's top left, from the fragment position:
	/// counting from the bottom when the quad faces front and from the top when
	/// it faces back. So the quad must face front on a target whose fragment
	/// positions count up from the bottom (OpenGL's default framebuffer), and
	/// back on one whose positions count down from the top (Metal).
	pub vertex: String,
	/// The fragment shader.
	pub fragment: String,
	/// Every uniform of both stages.
	pub uniforms: Vec<Uniform>,
	/// Bytes in the vertex stage's uniform block.
	pub vertex_block_size: u32,
	/// Bytes in the fragment stage's uniform block.
	pub fragment_block_size: u32,
	/// The texture slots, in order: slot `k` is the `k`th.
	pub textures: Vec<Texture>,
	/// Whether the material reads `sl_time`.
	pub reads_time: bool,
	/// Whether the material reads the device's tilt, orientation or rotation.
	pub reads_motion: bool,
}

/// One uniform.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Uniform {
	/// Its name, by which OpenGL ES sets it.
	pub name: String,
	/// Its type.
	pub kind: UniformKind,
	/// The stage that declares it.
	pub stage: Stage,
	/// Its byte offset in its stage's uniform block, where Metal writes it.
	pub offset: u32,
	/// What fills it.
	pub role: UniformRole,
}

/// A uniform's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum UniformKind {
	/// `float`
	Float,
	/// `int`
	Int,
	/// `vec2`
	Vec2,
	/// `vec3`
	Vec3,
	/// `vec4`
	Vec4,
	/// `mat3`
	Mat3,
}

/// A shader stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Stage {
	/// The vertex stage.
	Vertex,
	/// The fragment stage.
	Fragment,
}

/// What fills a uniform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum UniformRole {
	/// `sl_time`.
	Time,
	/// `sl_tilt`.
	Tilt,
	/// `sl_resolution`.
	Resolution,
	/// `sl_orientation`.
	Orientation,
	/// `sl_rotation`.
	Rotation,
	/// The layer's argument at this index, counting image arguments.
	Argument(u32),
	/// How the texture in this slot is read: 1 when it holds 32-bit floats the
	/// GPU cannot filter, else 0.
	SamplingMode(u32),
	/// 1 when drawing into a bottom-up target, -1 into a top-down one. It
	/// mirrors the quad, which also turns which way it faces (see `vertex`).
	TargetFlip,
}

/// One texture slot.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Texture {
	/// The sampler's name in the shader, by which OpenGL ES binds it.
	pub name: String,
	/// What fills it.
	pub role: TextureRole,
}

/// What fills a texture slot, and so how to sample it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TextureRole {
	/// The node's children: linear, clamped edges.
	Base,
	/// The previous layer's output, or the base for the first: linear, clamped
	/// edges.
	Input,
	/// The noise image: nearest, repeated.
	Noise,
	/// The layer's argument at this index: linear with clamped edges for an
	/// image, nearest and repeated for the noise image.
	Argument(u32),
}

/// The shaders for a material of `params` and `body`. Without
/// `float_filterable` they filter 32-bit float images themselves.
pub fn material_shader(
	params: &[Param],
	body: &str,
	language: ShaderLanguage,
	float_filterable: bool,
) -> Result<MaterialShader, MaterialFailure> {
	// The compiler joins a backslash-ended line to the next before reading
	// anything, and the format's body rules hold for the joined body.
	let body = &body.replace("\\\n", "");
	let fallback = !float_filterable;
	let typed = typed_pass(params, body).ok_or(MaterialFailure::Unsupported)?;
	let lowered = if fallback {
		fallback_pass(params, &typed).ok_or(MaterialFailure::Unsupported)?
	} else {
		typed
	};
	let words = tokenize(body).ok_or(MaterialFailure::Unsupported)?;
	let reads = |names: &[&str]| {
		words
			.iter()
			.any(|token| matches!(token, Token::Word(word) if names.contains(word)))
	};
	let textures = textures(params);
	let (uniforms, vertex_block_size, fragment_block_size) = uniforms(params, &textures, fallback);
	let (vertex, fragment) = match language {
		ShaderLanguage::GlslEs => (
			format!(
				"#version 300 es\nin vec2 position;out vec2 uv;\nuniform float _shaderlab_target_flip;\n{VERTEX_MAIN}"
			),
			fragment(params, &uniforms, &lowered, fallback),
		),
		ShaderLanguage::Metal => {
			let helpers = if fallback { fallback_helpers() } else { String::new() };
			metal_shaders(&format!("{helpers}\n{lowered}"), &uniforms, &textures)?
		},
	};
	Ok(MaterialShader {
		vertex,
		fragment,
		uniforms,
		vertex_block_size,
		fragment_block_size,
		textures,
		reads_time: reads(&["sl_time"]),
		reads_motion: reads(&["sl_tilt", "sl_orientation", "sl_rotation"]),
	})
}

/// The vertex stage's `main`, as Shaderlab writes it.
pub const VERTEX_MAIN: &str = "void main(){uv=vec2(position.x*.5+.5,.5-position.y*.5);gl_Position=vec4(position.x,position.y*_shaderlab_target_flip,0.,1.);}";

/// The fragment stage's `main`, as Shaderlab writes it.
pub const FRAGMENT_MAIN: &str = "void main(){vec2 uv=vec2(gl_FragCoord.x, gl_FrontFacing ? sl_resolution.y-gl_FragCoord.y : gl_FragCoord.y)/sl_resolution;result=material(uv);}";

fn fragment(params: &[Param], uniforms: &[Uniform], body: &str, fallback: bool) -> String {
	let arguments: Vec<String> = params
		.iter()
		.map(|param| match uniform_kind(param.kind) {
			Some(kind) => format!("uniform {} p_{};", kind.glsl(), param.name),
			None => format!("uniform highp sampler2D sp_{};", param.name),
		})
		.collect();
	let sampling = if fallback {
		let modes: Vec<String> = uniforms
			.iter()
			.filter(|uniform| matches!(uniform.role, UniformRole::SamplingMode(_)))
			.map(|uniform| format!("uniform int {};", uniform.name))
			.collect();
		format!("{}{}", modes.join("\n"), fallback_helpers())
	} else {
		String::new()
	};
	format!(
		"#version 300 es
precision highp float;precision highp sampler2D;in vec2 uv;out vec4 result;
uniform float sl_time;uniform vec2 sl_tilt;uniform vec2 sl_resolution;
uniform vec3 sl_orientation;uniform mat3 sl_rotation;

uniform highp sampler2D r_base;uniform highp sampler2D r_input;
uniform highp sampler2D r_noise;


{}




{sampling}
#line 1
{body}
{FRAGMENT_MAIN}",
		arguments.join("\n")
	)
}

fn mode_uniform(sampler: &str) -> String {
	format!("_shaderlab_sampling_{sampler}")
}

/// Uniforms of both stages, laid out by std140 in Shaderlab's order, and the
/// two blocks' sizes.
fn uniforms(params: &[Param], textures: &[Texture], fallback: bool) -> (Vec<Uniform>, u32, u32) {
	let vertex =
		[("_shaderlab_target_flip".to_string(), UniformKind::Float, UniformRole::TargetFlip)];
	let mut fragment = vec![
		("sl_time".to_string(), UniformKind::Float, UniformRole::Time),
		("sl_tilt".to_string(), UniformKind::Vec2, UniformRole::Tilt),
		("sl_resolution".to_string(), UniformKind::Vec2, UniformRole::Resolution),
		("sl_orientation".to_string(), UniformKind::Vec3, UniformRole::Orientation),
		("sl_rotation".to_string(), UniformKind::Mat3, UniformRole::Rotation),
	];
	fragment.extend((0u32..).zip(params).filter_map(|(index, param)| {
		Some((format!("p_{}", param.name), uniform_kind(param.kind)?, UniformRole::Argument(index)))
	}));
	if fallback {
		// Shaderlab's order: the input, then each image argument. The base and the
		// noise image are always 8-bit, so they take a literal mode.
		fragment.extend(
			textures
				.iter()
				.zip(0u32..)
				.filter(|(texture, _)| {
					matches!(texture.role, TextureRole::Input | TextureRole::Argument(_))
				})
				.map(|(texture, slot)| {
					(mode_uniform(&texture.name), UniformKind::Int, UniformRole::SamplingMode(slot))
				}),
		);
	}
	let (mut uniforms, vertex_size) = std140(Stage::Vertex, vertex.into_iter().collect());
	let (fragment, fragment_size) = std140(Stage::Fragment, fragment);
	uniforms.extend(fragment);
	(uniforms, vertex_size, fragment_size)
}

/// Members at their std140 offsets, and the block's size.
fn std140(stage: Stage, members: Vec<(String, UniformKind, UniformRole)>) -> (Vec<Uniform>, u32) {
	let mut end = 0u32;
	let uniforms = members
		.into_iter()
		.map(|(name, kind, role)| {
			let (align, size) = match kind {
				UniformKind::Float | UniformKind::Int => (4, 4),
				UniformKind::Vec2 => (8, 8),
				UniformKind::Vec3 => (16, 12),
				UniformKind::Vec4 => (16, 16),
				UniformKind::Mat3 => (16, 48),
			};
			let offset = end.next_multiple_of(align);
			end = offset + size;
			Uniform { name, kind, stage, offset, role }
		})
		.collect();
	(uniforms, end.next_multiple_of(16))
}

fn textures(params: &[Param]) -> Vec<Texture> {
	let texture = |name: &str, role| Texture { name: name.to_string(), role };
	let mut textures = vec![
		texture("r_base", TextureRole::Base),
		texture("r_input", TextureRole::Input),
		texture("r_noise", TextureRole::Noise),
	];
	textures.extend(
		(0u32..)
			.zip(params)
			.filter(|(_, param)| uniform_kind(param.kind).is_none())
			.map(|(index, param)| {
				texture(&format!("sp_{}", param.name), TextureRole::Argument(index))
			}),
	);
	textures
}

/// A number parameter's uniform type; `None` for an image parameter.
fn uniform_kind(kind: ParamKind) -> Option<UniformKind> {
	match kind {
		ParamKind::Float => Some(UniformKind::Float),
		ParamKind::Vec2 => Some(UniformKind::Vec2),
		ParamKind::Vec3 => Some(UniformKind::Vec3),
		ParamKind::Vec4 => Some(UniformKind::Vec4),
		ParamKind::SamplerFloat |
		ParamKind::SamplerVec2 |
		ParamKind::SamplerVec3 |
		ParamKind::SamplerVec4 => None,
	}
}

/// Shaderlab's sampler types for image parameters, and the channels a read of
/// each returns. A read of a four-channel image needs no swizzle.
const TYPED_SAMPLERS: [(&str, &str); 4] =
	[("samplerFloat", "r"), ("samplerVec2", "rg"), ("samplerVec3", "rgb"), ("samplerVec4", "")];

fn sampler_type(kind: ParamKind) -> &'static str {
	match kind {
		ParamKind::SamplerFloat => "samplerFloat",
		ParamKind::SamplerVec2 => "samplerVec2",
		ParamKind::SamplerVec3 => "samplerVec3",
		ParamKind::SamplerVec4 |
		ParamKind::Float |
		ParamKind::Vec2 |
		ParamKind::Vec3 |
		ParamKind::Vec4 => "samplerVec4",
	}
}

fn is_sampler_type(word: &str) -> bool {
	word == "sampler2D" || TYPED_SAMPLERS.iter().any(|(name, _)| *name == word)
}

/// The samplers a function body can name: its own sampler parameters, then
/// the global ones. Each comes with its type.
struct Samplers<'p, 'a> {
	params: &'p [Param],
	locals: Vec<(&'a str, &'a str)>,
}

impl<'a> Samplers<'_, 'a> {
	fn kind(&self, name: &str) -> Option<&'a str> {
		if let Some((_, kind)) = self.locals.iter().find(|(local, _)| *local == name) {
			return Some(kind);
		}
		match name {
			"r_base" | "r_input" => Some("sampler2D"),
			"r_noise" => Some("samplerFloat"),
			_ => self
				.params
				.iter()
				.find(|param| {
					uniform_kind(param.kind).is_none() &&
						name.strip_prefix("sp_") == Some(&param.name)
				})
				.map(|param| sampler_type(param.kind)),
		}
	}
}

fn function_samplers<'p, 'a>(params: &'p [Param], function: &Function<'a>) -> Samplers<'p, 'a> {
	let locals = function
		.params
		.iter()
		.filter(|param| is_sampler_type(param.kind))
		.filter_map(|param| Some((param.name?, param.kind)))
		.collect();
	Samplers { params, locals }
}

/// The typed-sampling pass: a read of a one-, two- or three-channel image
/// returns those channels, and Shaderlab's sampler types become `sampler2D`.
/// `None` when a name would hide a sampler, or for a sampler array.
fn typed_pass(params: &[Param], body: &str) -> Option<String> {
	let tokens = located_tokens(body)?;
	let mut edits = Vec::new();
	for function in functions(&tokens)? {
		let samplers = function_samplers(params, &function);
		let global = Samplers { params, locals: Vec::new() };
		let hides_a_sampler = function.params.iter().any(|param| {
			param.is_array && is_sampler_type(param.kind) ||
				!is_sampler_type(param.kind) &&
					param.name.is_some_and(|name| global.kind(name).is_some())
		});
		let Some(body) = function.body else {
			if hides_a_sampler {
				return None;
			}
			continue;
		};
		let calls = calls(&tokens, body.clone())?;
		// A sampler can only be handed to a function, so its name anywhere else
		// is something else's name hiding it.
		let handed_on: Vec<usize> = calls
			.iter()
			.flat_map(|(name, close)| split_arguments(&tokens[name + 2..*close]))
			.filter_map(bare_name)
			.map(|(at, _)| at)
			.collect();
		let hidden = body.clone().any(|index| {
			matches!(tokens[index], (at, Token::Word(name)) if samplers.kind(name).is_some() && !handed_on.contains(&at))
		});
		if hides_a_sampler || hidden {
			return None;
		}
		for (name, close) in calls {
			let Token::Word(function_name) = tokens[name].1 else { continue };
			if !SAMPLING_FUNCTIONS.contains(&function_name) {
				continue;
			}
			let arguments = split_arguments(&tokens[name + 2..close]);
			let channels = arguments
				.first()
				.and_then(|first| bare_name(first))
				.map(|(_, sampler)| sampler)
				.and_then(|sampler| samplers.kind(sampler))
				.and_then(|kind| TYPED_SAMPLERS.iter().find(|(name, _)| *name == kind))
				.map(|(_, channels)| *channels)
				.filter(|channels| !channels.is_empty());
			if let Some(channels) = channels {
				let start = tokens[name].0;
				let end = token_end(&tokens[close]);
				edits.push(Edit { start, end: start, text: "(".to_string() });
				edits.push(Edit { start: end, end, text: format!(".{channels})") });
			}
		}
	}
	for (at, token) in &tokens {
		if let Token::Word(word) = token &&
			TYPED_SAMPLERS.iter().any(|(name, _)| name == word)
		{
			edits.push(Edit { start: *at, end: at + word.len(), text: "sampler2D".to_string() });
		}
	}
	Some(apply(body, edits))
}

/// Calls the fallback leaves as they are: they read texels or sizes, never
/// filtered values. Every other sampling call goes through a helper.
const UNFILTERED: [&str; 3] = ["textureSize", "texelFetch", "texelFetchOffset"];

/// The float fallback pass, over the typed pass's output: sampling calls go
/// through the helpers, and every sampler carries its mode beside it.
fn fallback_pass(params: &[Param], typed: &str) -> Option<String> {
	let tokens = located_tokens(typed)?;
	let mut edits = Vec::new();
	for function in functions(&tokens)? {
		for param in function.params.iter().filter(|param| param.kind == "sampler2D") {
			let text = match param.name {
				Some(name) => format!(", int {}", mode_uniform(name)),
				None => ", int".to_string(),
			};
			edits.push(Edit { start: param.end, end: param.end, text });
		}
		let Some(body) = function.body.clone() else { continue };
		let samplers = function_samplers(params, &function);
		let mode = |name: &str| -> Option<String> {
			samplers.kind(name)?;
			Some(match name {
				"r_base" | "r_noise"
					if !samplers.locals.iter().any(|(local, _)| *local == name) =>
				{
					"0".to_string()
				},
				_ => mode_uniform(name),
			})
		};
		for (name, close) in calls(&tokens, body)? {
			let Token::Word(function_name) = tokens[name].1 else { continue };
			if UNFILTERED.contains(&function_name) {
				continue;
			}
			if SAMPLING_FUNCTIONS.contains(&function_name) {
				let start = tokens[name].0;
				edits.push(Edit {
					start,
					end: start + function_name.len(),
					text: format!("_shaderlab_{function_name}"),
				});
			}
			for argument in split_arguments(&tokens[name + 2..close]) {
				if let Some(mode) = bare_name(argument).and_then(|(_, sampler)| mode(sampler)) {
					let end = token_end(argument.last()?);
					edits.push(Edit { start: end, end, text: format!(", {mode}") });
				}
			}
		}
	}
	Some(apply(typed, edits))
}

/// Shaderlab's filtering helpers (`float-sampling.ts`), copied as written.
fn fallback_helpers() -> String {
	let projected: String = ["vec3", "vec4"]
		.iter()
		.map(|kind| {
			let divisor = if *kind == "vec3" { "z" } else { "w" };
			format!(
				"
vec4 _shaderlab_textureProj(sampler2D image, int mode, {kind} uv) {{
  return _shaderlab_texture(image, mode, uv.xy / uv.{divisor});
}}
vec4 _shaderlab_textureProj(sampler2D image, int mode, {kind} uv, float bias) {{
  return _shaderlab_texture(image, mode, uv.xy / uv.{divisor}, bias);
}}
vec4 _shaderlab_textureProjLod(sampler2D image, int mode, {kind} uv, float lod) {{
  return _shaderlab_textureLod(image, mode, uv.xy / uv.{divisor}, lod);
}}
vec4 _shaderlab_textureProjGrad(sampler2D image, int mode, {kind} uv, vec2 dx, vec2 dy) {{
  return _shaderlab_textureGrad(image, mode, uv.xy / uv.{divisor}, dx, dy);
}}
vec4 _shaderlab_textureProjOffset(sampler2D image, int mode, {kind} uv, ivec2 offset) {{
  return _shaderlab_texture(image, mode, uv.xy / uv.{divisor} + vec2(offset) / vec2(textureSize(image, 0)));
}}
vec4 _shaderlab_textureProjOffset(sampler2D image, int mode, {kind} uv, ivec2 offset, float bias) {{
  return _shaderlab_texture(image, mode, uv.xy / uv.{divisor} + vec2(offset) / vec2(textureSize(image, 0)), bias);
}}
vec4 _shaderlab_textureProjLodOffset(sampler2D image, int mode, {kind} uv, float lod, ivec2 offset) {{
  return _shaderlab_textureLod(image, mode, uv.xy / uv.{divisor} + vec2(offset) / vec2(textureSize(image, 0)), lod);
}}
vec4 _shaderlab_textureProjGradOffset(sampler2D image, int mode, {kind} uv, vec2 dx, vec2 dy, ivec2 offset) {{
  return _shaderlab_textureGrad(image, mode, uv.xy / uv.{divisor} + vec2(offset) / vec2(textureSize(image, 0)), dx, dy);
}}
"
			)
		})
		.collect();
	format!("{LINEAR_HELPERS}{projected}{OFFSET_HELPERS}")
}

const LINEAR_HELPERS: &str = "
ivec2 _shaderlab_address(ivec2 p, ivec2 size, int mode) {
  p.y = clamp(p.y, 0, size.y - 1);
  p.x = mode == 2 ? ((p.x % size.x) + size.x) % size.x : clamp(p.x, 0, size.x - 1);
  return p;
}
vec4 _shaderlab_linear(sampler2D image, vec2 uv, int mode) {
  ivec2 size = textureSize(image, 0);
  vec2 point = (mode == 2 ? vec2(fract(uv.x), clamp(uv.y, 0., 1.)) : clamp(uv, 0., 1.)) * vec2(size) - .5;
  ivec2 cell = ivec2(floor(point));
  vec2 weight = fract(point);
  vec4 a = texelFetch(image, _shaderlab_address(cell, size, mode), 0);
  vec4 b = texelFetch(image, _shaderlab_address(cell + ivec2(1, 0), size, mode), 0);
  vec4 c = texelFetch(image, _shaderlab_address(cell + ivec2(0, 1), size, mode), 0);
  vec4 d = texelFetch(image, _shaderlab_address(cell + ivec2(1, 1), size, mode), 0);
  return mix(mix(a, b, weight.x), mix(c, d, weight.x), weight.y);
}
vec4 _shaderlab_texture(sampler2D image, int mode, vec2 uv) {
  return mode == 0 ? texture(image, uv) : _shaderlab_linear(image, uv, mode);
}
vec4 _shaderlab_texture(sampler2D image, int mode, vec2 uv, float bias) {
  return mode == 0 ? texture(image, uv, bias) : _shaderlab_linear(image, uv, mode);
}
vec4 _shaderlab_textureLod(sampler2D image, int mode, vec2 uv, float lod) {
  return mode == 0 ? textureLod(image, uv, lod) : _shaderlab_linear(image, uv, mode);
}
vec4 _shaderlab_textureGrad(sampler2D image, int mode, vec2 uv, vec2 dx, vec2 dy) {
  return mode == 0 ? textureGrad(image, uv, dx, dy) : _shaderlab_linear(image, uv, mode);
}
";

const OFFSET_HELPERS: &str = "
vec4 _shaderlab_textureOffset(sampler2D image, int mode, vec2 uv, ivec2 offset) {
  return _shaderlab_texture(image, mode, uv + vec2(offset) / vec2(textureSize(image, 0)));
}
vec4 _shaderlab_textureOffset(sampler2D image, int mode, vec2 uv, ivec2 offset, float bias) {
  return _shaderlab_texture(image, mode, uv + vec2(offset) / vec2(textureSize(image, 0)), bias);
}
vec4 _shaderlab_textureLodOffset(sampler2D image, int mode, vec2 uv, float lod, ivec2 offset) {
  return _shaderlab_textureLod(image, mode, uv + vec2(offset) / vec2(textureSize(image, 0)), lod);
}
vec4 _shaderlab_textureGradOffset(sampler2D image, int mode, vec2 uv, vec2 dx, vec2 dy, ivec2 offset) {
  return _shaderlab_textureGrad(image, mode, uv + vec2(offset) / vec2(textureSize(image, 0)), dx, dy);
}
";

impl UniformKind {
	/// Its GLSL spelling.
	pub fn glsl(self) -> &'static str {
		match self {
			Self::Float => "float",
			Self::Int => "int",
			Self::Vec2 => "vec2",
			Self::Vec3 => "vec3",
			Self::Vec4 => "vec4",
			Self::Mat3 => "mat3",
		}
	}
}

#[cfg(test)]
mod tests;
