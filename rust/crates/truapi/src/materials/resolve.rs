//! A tree's materials, resolved for an app to draw: for each `Materials` node,
//! depth first, whether it is still loading, ready with its built layers, or
//! drawn plain and why.

use core::{
	pin::Pin,
	task::{Context, Poll},
};
use std::collections::{HashMap, VecDeque};

use futures::{
	FutureExt, Stream, StreamExt,
	channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded},
	future::{AbortHandle, Abortable},
	stream::{select, select_all},
};
use imagesize::ImageType;
use truapi::latest::{
	ImageRef, ImageSource, MaterialFailure, MaterialInstance, Parameter, RenderContext,
	RendererNode,
};

use super::{
	Materials,
	format::{Material, read_material},
	limits::tree_failures,
	materials_nodes,
	noise::noise_seed,
	shader::{MaterialShader, ShaderLanguage},
};

/// How an app draws: whether materials are on, its shader language, and
/// whether its GPU filters 32-bit float images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolveSettings {
	/// Whether the app draws materials at all.
	pub enabled: bool,
	/// The language the app compiles.
	pub language: ShaderLanguage,
	/// Whether the GPU filters 32-bit float images.
	pub float_filterable: bool,
}

/// A node that fell back.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NodeFallback {
	/// The node's place among the tree's `Materials` nodes, depth first.
	pub node: u32,
	/// The material that failed, when the failure belongs to one.
	pub material: Option<[u8; 32]>,
	/// Why the node fell back.
	pub reason: MaterialFailure,
}

/// What to draw for each `Materials` node of a tree.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ResolvedMaterials {
	/// One per `Materials` node, depth first.
	pub nodes: Vec<NodeMaterials>,
	/// The seed of the body's noise image.
	pub noise_seed: u32,
}

/// What to draw for one `Materials` node.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum NodeMaterials {
	/// Its materials are loading: draw its last look, or its children plain.
	Loading,
	/// Draw its layers, in order, over its children.
	Ready {
		/// The layers, in drawing order.
		layers: Vec<ReadyLayer>,
	},
	/// Draw its children plain.
	Plain {
		/// Why.
		reason: MaterialFailure,
	},
}

/// One layer ready to draw.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ReadyLayer {
	/// The material's ID.
	pub material: [u8; 32],
	/// Its shaders and how to feed them.
	pub shader: MaterialShader,
	/// The arguments the tree gave it, in declaration order.
	pub args: Vec<Parameter>,
}

/// A tree's resolved materials, again whenever a node changes. Open until
/// dropped, which stops what it fetches.
pub struct MaterialsResolution {
	updates: UnboundedReceiver<ResolvedMaterials>,
	reports: UnboundedSender<NodeFallback>,
	abort: AbortHandle,
}

impl MaterialsResolution {
	/// One update with every node of `tree` drawn plain, for a host that keeps
	/// no materials. Each node reaches `failures`.
	pub fn plain(
		tree: &RendererNode,
		context: &RenderContext,
		failures: Box<dyn Fn(NodeFallback) + Send + Sync>,
	) -> Self {
		let reason = MaterialFailure::Unsupported;
		let count = materials_nodes(tree).len();
		for node in (0u32..).take(count) {
			tell(&failures, NodeFallback { node, material: None, reason });
		}
		let resolved = ResolvedMaterials {
			nodes: vec![NodeMaterials::Plain { reason }; count],
			noise_seed: noise_seed(context),
		};
		let (sender, updates) = unbounded();
		let _ = sender.unbounded_send(resolved);
		let (reports, _) = unbounded();
		Self { updates, reports, abort: AbortHandle::new_pair().0 }
	}

	/// A handle that reports for this resolution while the stream is read elsewhere.
	pub fn reporter(&self) -> FallbackReporter {
		FallbackReporter(self.reports.clone())
	}
}

/// Reports for one resolution, kept apart from its stream.
pub struct FallbackReporter(UnboundedSender<NodeFallback>);

impl FallbackReporter {
	/// Makes a node plain for the rest of its resolution, as the app reports it. A
	/// node out of range is ignored.
	pub fn report(&self, failure: NodeFallback) {
		let _ = self.0.unbounded_send(failure);
	}
}

impl Stream for MaterialsResolution {
	type Item = ResolvedMaterials;

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		self.updates.poll_next_unpin(cx)
	}
}

impl Drop for MaterialsResolution {
	fn drop(&mut self) {
		self.abort.abort();
	}
}

impl Materials {
	/// The resolved materials of `tree` in `context`. Each node that falls back reaches
	/// `failures` once, with its first reason.
	pub fn resolve(
		&self,
		tree: &RendererNode,
		context: &RenderContext,
		settings: ResolveSettings,
		failures: Box<dyn Fn(NodeFallback) + Send + Sync>,
	) -> MaterialsResolution {
		let (sender, updates) = unbounded();
		let (reports, received) = unbounded();
		let (abort, registration) = AbortHandle::new_pair();
		let resolver = Resolver::new(self.clone(), tree, context, settings, failures);
		self.0
			.spawner
			.spawn(Abortable::new(resolver.run(sender, received), registration).map(drop));
		MaterialsResolution { updates, reports, abort }
	}
}

/// Whether an image argument can be drawn: PNG, JPEG or WebP, at most 2048
/// pixels a side. `None` when it can.
pub fn check_material_image(bytes: &[u8]) -> Option<MaterialFailure> {
	if !matches!(
		imagesize::image_type(bytes),
		Ok(ImageType::Png | ImageType::Jpeg | ImageType::Webp)
	) {
		return Some(MaterialFailure::InvalidStack);
	}
	match imagesize::blob_size(bytes) {
		Ok(size) if size.width <= MAX_IMAGE_SIDE && size.height <= MAX_IMAGE_SIDE => None,
		Ok(_) => Some(MaterialFailure::LimitExceeded),
		Err(_) => Some(MaterialFailure::InvalidStack),
	}
}

/// Pixels a side of an image argument, at most.
const MAX_IMAGE_SIDE: usize = 2048;

/// One `Materials` node as the resolver follows it.
struct TrackedNode {
	layers: Vec<MaterialInstance>,
	/// A failure no material can undo: a limit, the surface, an inline image,
	/// or the app's report.
	settled: Option<(Option<[u8; 32]>, MaterialFailure)>,
	reported: bool,
}

/// What a resolver knows and has told so far.
struct Resolver {
	materials: Materials,
	settings: ResolveSettings,
	failures: Box<dyn Fn(NodeFallback) + Send + Sync>,
	noise_seed: u32,
	nodes: Vec<TrackedNode>,
	texts: HashMap<[u8; 32], Result<Material, MaterialFailure>>,
}

enum Event {
	Text([u8; 32], Result<Vec<u8>, MaterialFailure>),
	Report(NodeFallback),
}

impl Resolver {
	fn new(
		materials: Materials,
		tree: &RendererNode,
		context: &RenderContext,
		settings: ResolveSettings,
		failures: Box<dyn Fn(NodeFallback) + Send + Sync>,
	) -> Self {
		let drawn_here = settings.enabled &&
			matches!(
				context,
				RenderContext::PocketCard { .. } | RenderContext::PocketCardBack { .. }
			);
		let nodes = materials_nodes(tree)
			.into_iter()
			.zip(tree_failures(tree))
			.map(|((props, _), limit)| {
				let settled = if !drawn_here {
					Some((None, MaterialFailure::Unsupported))
				} else if let Some(failure) = limit {
					Some((None, failure))
				} else {
					inline_image_failure(&props.layers)
				};
				TrackedNode { layers: props.layers.clone(), settled, reported: false }
			})
			.collect();
		Self {
			materials,
			settings,
			failures,
			noise_seed: noise_seed(context),
			nodes,
			texts: HashMap::new(),
		}
	}

	async fn run(
		mut self,
		updates: UnboundedSender<ResolvedMaterials>,
		reports: UnboundedReceiver<NodeFallback>,
	) {
		let mut missing = Vec::new();
		for id in self.wanted() {
			match self.materials.kept(id).await {
				Ok(Some(text)) => self.apply(Event::Text(id, Ok(text))),
				_ => missing.push(id),
			}
		}
		let mut last = self.current();
		if updates.unbounded_send(last.clone()).is_err() {
			return;
		}
		let texts = select_all(
			missing
				.into_iter()
				.map(|id| self.materials.observe(id).map(move |text| Event::Text(id, text))),
		);
		let mut events = select(texts, reports.map(Event::Report));
		while let Some(event) = events.next().await {
			self.apply(event);
			// Answers already waiting arrive in the same update.
			while let Some(Some(event)) = events.next().now_or_never() {
				self.apply(event);
			}
			let resolved = self.current();
			if resolved != last {
				if updates.unbounded_send(resolved.clone()).is_err() {
					return;
				}
				last = resolved;
			}
		}
	}

	/// The materials of nodes that can still draw, each once.
	fn wanted(&self) -> Vec<[u8; 32]> {
		let mut ids = Vec::new();
		for node in self.nodes.iter().filter(|node| node.settled.is_none()) {
			for layer in &node.layers {
				if !ids.contains(&layer.material) {
					ids.push(layer.material);
				}
			}
		}
		ids
	}

	fn apply(&mut self, event: Event) {
		match event {
			Event::Text(id, text) => {
				let material =
					text.and_then(|text| read_material(&text).map_err(MaterialFailure::from));
				self.texts.insert(id, material);
			},
			Event::Report(failure) => {
				if let Some(node) = self.nodes.get_mut(failure.node as usize) &&
					node.settled.is_none()
				{
					node.settled = Some((failure.material, failure.reason));
				}
			},
		}
	}

	/// The materials as they stand. Nodes plain for the first time are logged and
	/// told to `failures`.
	fn current(&mut self) -> ResolvedMaterials {
		let mut nodes = Vec::with_capacity(self.nodes.len());
		for index in 0..self.nodes.len() {
			let resolved = self.node_materials(index);
			let node = &mut self.nodes[index];
			if let (NodeMaterials::Plain { reason }, false) = (&resolved.0, node.reported) {
				node.reported = true;
				let failure =
					NodeFallback { node: index as u32, material: resolved.1, reason: *reason };
				tell(&self.failures, failure);
			}
			nodes.push(resolved.0);
		}
		ResolvedMaterials { nodes, noise_seed: self.noise_seed }
	}

	/// A node's materials, and the material its failure belongs to.
	fn node_materials(&self, index: usize) -> (NodeMaterials, Option<[u8; 32]>) {
		let node = &self.nodes[index];
		if let Some((material, reason)) = node.settled {
			return (NodeMaterials::Plain { reason }, material);
		}
		let mut layers = Vec::with_capacity(node.layers.len());
		let mut loading = false;
		for layer in &node.layers {
			match self.layer(layer) {
				Some(Ok(ready)) => layers.push(ready),
				Some(Err(reason)) => {
					return (NodeMaterials::Plain { reason }, Some(layer.material));
				},
				None => loading = true,
			}
		}
		if loading {
			(NodeMaterials::Loading, None)
		} else {
			(NodeMaterials::Ready { layers }, None)
		}
	}

	/// A layer built, its failure, or `None` while its material loads.
	fn layer(&self, layer: &MaterialInstance) -> Option<Result<ReadyLayer, MaterialFailure>> {
		let material = match self.texts.get(&layer.material)? {
			Ok(material) => material,
			Err(failure) => return Some(Err(*failure)),
		};
		if !material.arguments_match(&layer.args) {
			return Some(Err(MaterialFailure::InvalidStack));
		}
		let ResolveSettings { language, float_filterable, .. } = self.settings;
		let key = (layer.material, language, float_filterable);
		let kept = self.materials.0.shaders.lock().get(&key);
		// Built outside the lock: a translation would hold up every other resolution.
		let shader = kept.unwrap_or_else(|| {
			let built = material.shader(language, float_filterable);
			self.materials.0.shaders.lock().insert(key, built.clone());
			built
		});
		Some(shader.map(|shader| ReadyLayer {
			material: layer.material,
			shader,
			args: layer.args.clone(),
		}))
	}
}

/// The failure of the first inline image argument no host can draw.
fn inline_image_failure(
	layers: &[MaterialInstance],
) -> Option<(Option<[u8; 32]>, MaterialFailure)> {
	layers.iter().find_map(|layer| {
		layer.args.iter().find_map(|argument| match argument {
			Parameter::SamplerFloat(ImageRef::Source(ImageSource::Inline(bytes))) |
			Parameter::SamplerVec2(ImageRef::Source(ImageSource::Inline(bytes))) |
			Parameter::SamplerVec3(ImageRef::Source(ImageSource::Inline(bytes))) |
			Parameter::SamplerVec4(ImageRef::Source(ImageSource::Inline(bytes))) => {
				check_material_image(bytes).map(|failure| (Some(layer.material), failure))
			},
			_ => None,
		})
	})
}

/// Built shaders kept at most.
const SHADER_CACHE_CAPACITY: usize = 64;

/// A material's ID, the language and whether the GPU filters float images.
type ShaderKey = ([u8; 32], ShaderLanguage, bool);

/// Logs a node drawn plain on the device and tells `failures`.
fn tell(failures: &dyn Fn(NodeFallback), failure: NodeFallback) {
	tracing::info!(
		target: crate::markers::MATERIALS_NODE_PLAIN.target,
		"node {} draws plain: {:?}, material {:?}",
		failure.node,
		failure.reason,
		failure.material.map(hex::encode)
	);
	failures(failure);
}

/// What building each shader gave, failures included, the oldest dropped
/// first.
#[derive(Default)]
pub struct ShaderCache {
	shaders: VecDeque<(ShaderKey, Result<MaterialShader, MaterialFailure>)>,
}

impl ShaderCache {
	/// What building `key` gave, when kept.
	pub fn get(&self, key: &ShaderKey) -> Option<Result<MaterialShader, MaterialFailure>> {
		self.shaders
			.iter()
			.find(|(kept, _)| kept == key)
			.map(|(_, built)| built.clone())
	}

	/// Keeps what building `key` gave, in place of what it gave before,
	/// dropping the oldest beyond capacity.
	pub fn insert(&mut self, key: ShaderKey, built: Result<MaterialShader, MaterialFailure>) {
		self.shaders.retain(|(kept, _)| kept != &key);
		self.shaders.push_back((key, built));
		if self.shaders.len() > SHADER_CACHE_CAPACITY {
			self.shaders.pop_front();
		}
	}
}

#[cfg(test)]
mod tests;
