//! Limits on the work one tree asks of a host, checked on the tree alone,
//! before anything loads, so every host falls back at the same nodes.

use std::collections::HashSet;

use truapi::latest::{
	ImageRef, ImageSource, MaterialFailure, MaterialsProps, Parameter, RendererNode,
};

use super::{PlacedMaterials, materials_nodes};

const MAX_NODE_LAYERS: usize = 16;
const MAX_TREE_LAYERS: usize = 32;
const MAX_TREE_MATERIALS: usize = 16;
const MAX_IMAGE_ARGUMENTS: usize = 13;
const MAX_TREE_IMAGES: usize = 16;
const MAX_BLEED: u64 = 64;

/// Why each `Materials` node in `tree` falls back before anything loads, if
/// it does, depth first. A node that falls back adds nothing to the tree's
/// totals, so later nodes are decided the same way on every host.
pub fn tree_failures(tree: &RendererNode) -> Vec<Option<MaterialFailure>> {
	let mut budget = TreeBudget::default();
	let mut failures = Vec::new();
	for node in materials_nodes(tree) {
		failures.push(check_node(&node).and_then(|()| budget.admit(node.props)).err());
	}
	failures
}

/// The limits a node meets on its own, and whether a `Touch` node feeds the
/// touch arguments it takes.
fn check_node(node: &PlacedMaterials<'_>) -> Result<(), MaterialFailure> {
	let props = node.props;
	if node.nested {
		return Err(MaterialFailure::LimitExceeded);
	}
	if props.layers.is_empty() {
		return Err(MaterialFailure::InvalidStack);
	}
	let too_many_images = props
		.layers
		.iter()
		.any(|layer| image_arguments(&layer.args).count() > MAX_IMAGE_ARGUMENTS);
	if props.layers.len() > MAX_NODE_LAYERS || too_many_images || props.bleed.0 > MAX_BLEED {
		return Err(MaterialFailure::LimitExceeded);
	}
	let arguments = props.layers.iter().flat_map(|layer| &layer.args);
	let touch_fed = |argument: &Parameter| match argument {
		Parameter::Touch | Parameter::TouchState => node.touch.is_some(),
		Parameter::Replay => node.touch.is_some_and(|touch| touch.replay.is_some()),
		Parameter::SamplerFloat(ImageRef::Trail) |
		Parameter::SamplerVec2(ImageRef::Trail) |
		Parameter::SamplerVec3(ImageRef::Trail) |
		Parameter::SamplerVec4(ImageRef::Trail) => node.touch.is_some_and(|touch| touch.trail.is_some()),
		_ => true,
	};
	if !arguments.into_iter().all(touch_fed) {
		return Err(MaterialFailure::InvalidStack);
	}
	Ok(())
}

/// What the nodes admitted so far add up to.
#[derive(Default)]
struct TreeBudget<'tree> {
	layers: usize,
	materials: HashSet<[u8; 32]>,
	images: Vec<&'tree ImageSource>,
}

impl<'tree> TreeBudget<'tree> {
	/// Adds the node to the totals if it fits, and nothing otherwise.
	fn admit(&mut self, props: &'tree MaterialsProps) -> Result<(), MaterialFailure> {
		let layers = self.layers + props.layers.len();
		let new_materials: HashSet<[u8; 32]> = props
			.layers
			.iter()
			.map(|layer| layer.material)
			.filter(|material| !self.materials.contains(material))
			.collect();
		let mut new_images = Vec::new();
		for image in image_sources(props) {
			if !self.images.contains(&image) && !new_images.contains(&image) {
				new_images.push(image);
			}
		}
		if layers > MAX_TREE_LAYERS ||
			self.materials.len() + new_materials.len() > MAX_TREE_MATERIALS ||
			self.images.len() + new_images.len() > MAX_TREE_IMAGES
		{
			return Err(MaterialFailure::LimitExceeded);
		}
		self.layers = layers;
		self.materials.extend(new_materials);
		self.images.extend(new_images);
		Ok(())
	}
}

fn image_arguments(args: &[Parameter]) -> impl Iterator<Item = &ImageRef> {
	args.iter().filter_map(|argument| match argument {
		Parameter::SamplerFloat(image) |
		Parameter::SamplerVec2(image) |
		Parameter::SamplerVec3(image) |
		Parameter::SamplerVec4(image) => Some(image),
		Parameter::Float(_) |
		Parameter::Vec2(_) |
		Parameter::Vec3(_) |
		Parameter::Vec4(_) |
		Parameter::Touch |
		Parameter::TouchState |
		Parameter::Elapsed |
		Parameter::Replay => None,
	})
}

/// The images a node's layers load. The noise image and the trail are made,
/// not loaded.
fn image_sources(props: &MaterialsProps) -> impl Iterator<Item = &ImageSource> {
	props
		.layers
		.iter()
		.flat_map(|layer| image_arguments(&layer.args))
		.filter_map(|image| match image {
			ImageRef::Source(source) => Some(source),
			ImageRef::Noise | ImageRef::Trail => None,
		})
}

#[cfg(test)]
mod tests;
