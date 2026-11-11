use std::sync::{Arc, Mutex};

use futures::{FutureExt, StreamExt};
use truapi::latest::{
	BoxProps, ImageRef, ImageSource, MaterialFailure, MaterialInstance, MaterialsProps, Parameter,
	RenderContext, RendererNode,
};

use super::*;
use crate::{
	host_internal::bulletin::preimage_key,
	materials::{
		format::read_material,
		noise::noise_seed,
		testing::{Device, next},
	},
	store::open_temporary_core_db,
	test_support::wait_until,
};

const STILL: &[u8] =
	b"// @material 1\nvec4 material(vec2 uv) {\n  return texture(r_input, uv);\n}\n";

const TINT: &[u8] = b"// @material 1\n// @control {\"name\":\"k\",\"label\":\"K\",\"type\":\"float\",\"default\":1}\nvec4 material(vec2 uv) {\n  return texture(r_input, uv) * p_k;\n}\n";

const HEIGHT: &[u8] = b"// @material 1\n// @control {\"name\":\"height\",\"label\":\"Height\",\"type\":\"samplerFloat\"}\nvec4 material(vec2 uv) {\n  return texture(r_input, uv) * texture(sp_height, uv);\n}\n";

const BROKEN: &[u8] = b"// @material 1\nfloat shade(vec2 uv) {\n  return 1.0;\n}\n";

const NEWER: &[u8] =
	b"// @material 2\nvec4 material(vec2 uv) {\n  return texture(r_input, uv);\n}\n";

fn tint_args() -> Vec<Parameter> {
	vec![Parameter::Float(1f32.to_bits())]
}

fn layer(text: &[u8], args: Vec<Parameter>) -> MaterialInstance {
	MaterialInstance { material: preimage_key(text), args }
}

fn node(layers: Vec<MaterialInstance>) -> RendererNode {
	RendererNode::Materials { props: MaterialsProps { layers }, children: Vec::new() }
}

fn tree(nodes: Vec<RendererNode>) -> RendererNode {
	RendererNode::Box {
		modifiers: Vec::new(),
		props: BoxProps { content_alignment: None },
		children: nodes,
	}
}

fn card() -> RenderContext {
	RenderContext::PocketCard { card_id: "card".to_string() }
}

const ON: PlanSettings =
	PlanSettings { enabled: true, language: ShaderLanguage::GlslEs, float_filterable: true };

/// A node's state without its built layers.
#[derive(Debug, PartialEq)]
enum State {
	Loading,
	Ready,
	Plain(MaterialFailure),
}

fn states(plan: &MaterialPlan) -> Vec<State> {
	plan.nodes
		.iter()
		.map(|node| match node {
			NodePlan::Loading => State::Loading,
			NodePlan::Ready { .. } => State::Ready,
			NodePlan::Plain { reason } => State::Plain(*reason),
		})
		.collect()
}

struct Planning {
	plans: PlanStream,
	failures: Arc<Mutex<Vec<PlanFailure>>>,
}

impl Planning {
	fn next(&mut self) -> Vec<State> {
		states(&next(&mut self.plans).expect("a plan"))
	}

	fn failures(&self) -> Vec<PlanFailure> {
		self.failures.lock().unwrap().clone()
	}
}

fn plan(
	device: &Device,
	tree: &RendererNode,
	context: &RenderContext,
	settings: PlanSettings,
) -> Planning {
	let failures = Arc::new(Mutex::new(Vec::new()));
	let sink = failures.clone();
	let plans = device.materials.plan(
		tree,
		context,
		settings,
		Box::new(move |failure| sink.lock().unwrap().push(failure)),
	);
	Planning { plans, failures }
}

/// Answers `text` from Bulletin once the plan has looked it up.
fn answer(device: &Device, text: &[u8]) {
	let id = preimage_key(text);
	wait_until(|| device.bulletin.open_lookups(id) == 1, "the material is looked up");
	device.bulletin.answer(id, text.to_vec());
}

fn fail(node: u32, material: &[u8], reason: MaterialFailure) -> PlanFailure {
	PlanFailure { node, material: Some(preimage_key(material)), reason }
}

/// The limits are checked on the tree before anything loads, so a node over
/// one falls back at once and costs Bulletin nothing.
#[test]
fn a_node_over_a_limit_falls_back_at_once_and_fetches_nothing() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let layers = (0..17).map(|_| layer(STILL, Vec::new())).collect();
	let mut planning = plan(&device, &tree(vec![node(layers)]), &card(), ON);

	assert_eq!(planning.next(), [State::Plain(MaterialFailure::LimitExceeded)]);
	assert_eq!(device.bulletin.lookups(preimage_key(STILL)), 0);
	assert_eq!(
		planning.failures(),
		[PlanFailure { node: 0, material: None, reason: MaterialFailure::LimitExceeded }]
	);
}

/// An inline image is already in the tree, so the core checks it as it plans,
/// and a node with an image no host can use never loads.
#[test]
fn a_node_with_an_unusable_inline_image_falls_back_at_once() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let image = Parameter::SamplerFloat(ImageRef::Source(ImageSource::Inline(vec![1, 2, 3])));
	let mut planning =
		plan(&device, &tree(vec![node(vec![layer(HEIGHT, vec![image])])]), &card(), ON);

	assert_eq!(planning.next(), [State::Plain(MaterialFailure::InvalidStack)]);
	assert_eq!(device.bulletin.lookups(preimage_key(HEIGHT)), 0);
}

/// R2 draws materials on Pocket cards only, front and back, and nowhere when
/// the app has them off. Elsewhere a node draws plain as unsupported.
#[test]
fn materials_draw_only_on_pocket_cards_and_only_when_on() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let face = tree(vec![node(vec![layer(STILL, Vec::new())])]);
	let chat = RenderContext::ChatMessage {
		room_id: "room".to_string(),
		message_id: "message".to_string(),
		message_type: "note".to_string(),
	};
	let off = PlanSettings { enabled: false, ..ON };

	for (context, settings) in [(chat, ON), (card(), off)] {
		let mut planning = plan(&device, &face, &context, settings);
		assert_eq!(planning.next(), [State::Plain(MaterialFailure::Unsupported)], "{context:?}");
	}
	assert_eq!(device.bulletin.lookups(preimage_key(STILL)), 0);

	let back = RenderContext::PocketCardBack { card_id: "card".to_string() };
	assert_eq!(plan(&device, &face, &back, ON).next(), [State::Loading]);
}

/// A ready node carries exactly the shaders part 4 builds for its materials
/// and the arguments the tree gave them. A plan that would not change is not
/// sent, so the node goes from loading straight to ready.
#[test]
fn a_node_is_ready_once_every_material_arrives_with_the_shaders_built_for_it() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let layers = vec![layer(STILL, Vec::new()), layer(TINT, tint_args())];
	let mut planning = plan(&device, &tree(vec![node(layers)]), &card(), ON);
	assert_eq!(planning.next(), [State::Loading]);

	answer(&device, STILL);
	answer(&device, TINT);

	let built = |text: &[u8], args| PlannedLayer {
		material: preimage_key(text),
		shader: read_material(text).unwrap().shader(ShaderLanguage::GlslEs, true).unwrap(),
		args,
	};
	assert_eq!(
		next(&mut planning.plans),
		Some(MaterialPlan {
			nodes: vec![NodePlan::Ready {
				layers: vec![built(STILL, Vec::new()), built(TINT, tint_args())]
			}],
			noise_seed: noise_seed(&card()),
		})
	);
	assert!(planning.failures().is_empty());
}

/// A card whose materials are on the device draws them in its first frame:
/// the plan reads the device before it answers, rather than sending a loading
/// plan first.
#[test]
fn a_tree_whose_materials_are_kept_arrives_ready_in_one_plan() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	device.keep(STILL);

	let mut planning =
		plan(&device, &tree(vec![node(vec![layer(STILL, Vec::new())])]), &card(), ON);

	assert_eq!(planning.next(), [State::Ready]);
}

/// A card heals by itself: a node that fell back at the deadline draws once
/// its material arrives, but its product hears about it only once.
#[test]
fn a_material_missing_at_the_deadline_falls_back_then_heals() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let mut planning =
		plan(&device, &tree(vec![node(vec![layer(STILL, Vec::new())])]), &card(), ON);
	assert_eq!(planning.next(), [State::Loading]);

	device.pass_the_deadline();
	assert_eq!(planning.next(), [State::Plain(MaterialFailure::NotFound)]);
	answer(&device, STILL);

	assert_eq!(planning.next(), [State::Ready]);
	assert_eq!(planning.failures(), [fail(0, STILL, MaterialFailure::NotFound)]);
}

/// A material this core cannot read falls back with the reason a product can
/// act on: broken, or newer than this host.
#[test]
fn a_material_the_core_cannot_read_falls_back_with_its_reason() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	for (text, reason) in
		[(BROKEN, MaterialFailure::InvalidMaterial), (NEWER, MaterialFailure::Unsupported)]
	{
		let mut planning =
			plan(&device, &tree(vec![node(vec![layer(text, Vec::new())])]), &card(), ON);
		assert_eq!(planning.next(), [State::Loading]);
		wait_until(
			|| device.bulletin.lookups(preimage_key(text)) == 1,
			"the material is looked up",
		);

		answer(&device, text);

		assert_eq!(planning.next(), [State::Plain(reason)]);
	}
}

/// Arguments belong to a layer, not to a material, so wrong arguments fail only
/// the node that gave them.
#[test]
fn arguments_that_do_not_fit_fail_their_node_only() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let nodes = vec![node(vec![layer(TINT, Vec::new())]), node(vec![layer(TINT, tint_args())])];
	let mut planning = plan(&device, &tree(nodes), &card(), ON);
	assert_eq!(planning.next(), [State::Loading, State::Loading]);

	answer(&device, TINT);

	assert_eq!(planning.next(), [State::Plain(MaterialFailure::InvalidStack), State::Ready]);
	assert_eq!(planning.failures(), [fail(0, TINT, MaterialFailure::InvalidStack)]);
}

#[test]
fn a_material_named_by_two_nodes_is_fetched_once() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let nodes = vec![node(vec![layer(STILL, Vec::new())]), node(vec![layer(STILL, Vec::new())])];
	let mut planning = plan(&device, &tree(nodes), &card(), ON);
	assert_eq!(planning.next(), [State::Loading, State::Loading]);

	answer(&device, STILL);

	assert_eq!(planning.next(), [State::Ready, State::Ready]);
	assert_eq!(device.bulletin.lookups(preimage_key(STILL)), 1);
}

/// The app's report is final for the plan: a shader the device could not
/// compile is never handed back to it, even when its material arrives again.
#[test]
fn an_app_report_makes_its_node_plain_for_good() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let nodes = vec![node(vec![layer(STILL, Vec::new())]), node(vec![layer(TINT, tint_args())])];
	let mut planning = plan(&device, &tree(nodes), &card(), ON);
	assert_eq!(planning.next(), [State::Loading, State::Loading]);

	planning.plans.report(fail(1, TINT, MaterialFailure::Unsupported));
	assert_eq!(planning.next(), [State::Loading, State::Plain(MaterialFailure::Unsupported)]);
	answer(&device, TINT);
	answer(&device, STILL);
	assert_eq!(planning.next(), [State::Ready, State::Plain(MaterialFailure::Unsupported)]);

	planning.plans.report(fail(0, STILL, MaterialFailure::NotFound));
	planning.plans.report(fail(7, STILL, MaterialFailure::NotFound));
	assert_eq!(
		planning.next(),
		[State::Plain(MaterialFailure::NotFound), State::Plain(MaterialFailure::Unsupported)],
		"a report after every node is ready still lands, and one out of range changes nothing"
	);
	assert_eq!(
		planning.failures(),
		[fail(1, TINT, MaterialFailure::Unsupported), fail(0, STILL, MaterialFailure::NotFound)]
	);
}

/// A product re-rendering a card gets a new plan for the new tree. Asked
/// before the old one is cancelled, the material keeps its one fetch.
#[test]
fn a_new_plan_asked_before_the_old_is_cancelled_keeps_the_fetch() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let face = tree(vec![node(vec![layer(STILL, Vec::new())])]);
	let mut old = plan(&device, &face, &card(), ON);
	assert_eq!(old.next(), [State::Loading]);
	wait_until(
		|| device.bulletin.open_lookups(preimage_key(STILL)) == 1,
		"the material is looked up",
	);

	let mut new = plan(&device, &face, &card(), ON);
	assert_eq!(new.next(), [State::Loading]);
	drop(old);
	answer(&device, STILL);

	assert_eq!(new.next(), [State::Ready]);
	assert_eq!(device.bulletin.lookups(preimage_key(STILL)), 1);
}

/// A plan stays open after every node settles, so the app's reports still
/// land, and cancelling it stops what it fetches.
#[test]
fn a_plan_stays_open_until_cancelled_and_cancelling_stops_its_fetches() {
	let (_dir, db) = open_temporary_core_db();
	let device = Device::over(db);
	let face =
		tree(vec![node(vec![layer(STILL, Vec::new())]), node(vec![layer(TINT, tint_args())])]);
	let mut planning = plan(&device, &face, &card(), ON);
	assert_eq!(planning.next(), [State::Loading, State::Loading]);
	answer(&device, STILL);
	assert_eq!(planning.next(), [State::Ready, State::Loading]);

	assert!(planning.plans.next().now_or_never().is_none(), "the plan is still open");
	drop(planning);

	wait_until(|| device.bulletin.open_lookups(preimage_key(TINT)) == 0, "the lookup stops");
}

impl ShaderCache {
	fn len(&self) -> usize {
		self.shaders.len()
	}

	fn contains(&self, key: &ShaderKey) -> bool {
		self.shaders.iter().any(|(kept, _)| kept == key)
	}
}

/// A session can see many materials, so built shaders are kept within a fixed
/// number, the oldest dropped first.
#[test]
fn the_shader_cache_keeps_at_most_its_capacity_dropping_the_oldest() {
	let mut cache = ShaderCache::default();
	let shader = || read_material(STILL).unwrap().shader(ShaderLanguage::GlslEs, true);
	for index in 0..=SHADER_CACHE_CAPACITY {
		let key = ([index as u8; 32], ShaderLanguage::GlslEs, true);
		cache.get_or_build(key, shader).unwrap();
	}

	assert_eq!(cache.len(), SHADER_CACHE_CAPACITY);
	assert!(!cache.contains(&([0; 32], ShaderLanguage::GlslEs, true)), "the oldest went");
	assert!(cache.contains(&([1; 32], ShaderLanguage::GlslEs, true)));
}

/// One rule for every image argument, wherever the app loaded it from.
#[test]
fn an_image_argument_must_be_png_jpeg_or_webp_and_at_most_2048_pixels_a_side() {
	let png = |width: u32, height: u32| {
		let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
		bytes.extend(width.to_be_bytes());
		bytes.extend(height.to_be_bytes());
		bytes.extend([8, 6, 0, 0, 0]);
		bytes
	};
	let gif = b"GIF89a\x10\x00\x10\x00\x00\x00\x00".to_vec();
	let jpeg =
		b"\xff\xd8\xff\xc0\x00\x11\x08\x00\x10\x00\x10\x03\x01\x22\x00\x02\x11\x01\x03\x11\x01"
			.to_vec();
	let webp =
		b"RIFF\x16\x00\x00\x00WEBPVP8X\x0a\x00\x00\x00\x00\x00\x00\x00\x0f\x00\x00\x0f\x00\x00"
			.to_vec();

	assert_eq!(check_material_image(&png(2048, 16)), None);
	assert_eq!(check_material_image(&jpeg), None);
	assert_eq!(check_material_image(&webp), None);
	assert_eq!(check_material_image(&png(2049, 16)), Some(MaterialFailure::LimitExceeded));
	assert_eq!(check_material_image(&gif), Some(MaterialFailure::InvalidStack));
	assert_eq!(check_material_image(&[1, 2, 3]), Some(MaterialFailure::InvalidStack));
}
