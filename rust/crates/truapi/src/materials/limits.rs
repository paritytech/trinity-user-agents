//! Limits on the work one tree asks of a host, checked on the tree alone,
//! before anything loads, so every host falls back at the same nodes.

use std::collections::HashSet;

use truapi::latest::{
	ImageRef, ImageSource, MaterialFailure, MaterialsProps, Parameter, RendererNode,
};

use super::materials_nodes;

const MAX_NODE_LAYERS: usize = 16;
const MAX_TREE_LAYERS: usize = 32;
const MAX_TREE_MATERIALS: usize = 16;
const MAX_IMAGE_ARGUMENTS: usize = 13;
const MAX_TREE_IMAGES: usize = 16;

/// Why each `Materials` node in `tree` falls back before anything loads, if
/// it does, depth first. A node that falls back adds nothing to the tree's
/// totals, so later nodes are decided the same way on every host.
pub fn tree_failures(tree: &RendererNode) -> Vec<Option<MaterialFailure>> {
	let mut layers = 0;
	let mut materials = HashSet::new();
	let mut images: Vec<&ImageSource> = Vec::new();
	materials_nodes(tree)
		.into_iter()
		.map(|(props, nested)| {
			if nested {
				return Some(MaterialFailure::LimitExceeded);
			}
			if props.layers.is_empty() {
				return Some(MaterialFailure::InvalidStack);
			}
			let too_many_images = props
				.layers
				.iter()
				.any(|layer| image_arguments(&layer.args).count() > MAX_IMAGE_ARGUMENTS);
			if props.layers.len() > MAX_NODE_LAYERS || too_many_images {
				return Some(MaterialFailure::LimitExceeded);
			}
			let mut node_materials = materials.clone();
			node_materials.extend(props.layers.iter().map(|layer| layer.material));
			let mut node_images = images.clone();
			for image in image_sources(props) {
				if !node_images.contains(&image) {
					node_images.push(image);
				}
			}
			if layers + props.layers.len() > MAX_TREE_LAYERS ||
				node_materials.len() > MAX_TREE_MATERIALS ||
				node_images.len() > MAX_TREE_IMAGES
			{
				return Some(MaterialFailure::LimitExceeded);
			}
			layers += props.layers.len();
			materials = node_materials;
			images = node_images;
			None
		})
		.collect()
}

fn image_arguments(args: &[Parameter]) -> impl Iterator<Item = &ImageRef> {
	args.iter().filter_map(|argument| match argument {
		Parameter::SamplerFloat(image) |
		Parameter::SamplerVec2(image) |
		Parameter::SamplerVec3(image) |
		Parameter::SamplerVec4(image) => Some(image),
		Parameter::Float(_) | Parameter::Vec2(_) | Parameter::Vec3(_) | Parameter::Vec4(_) => None,
	})
}

/// The images a node's layers load. The noise image is made, not loaded.
fn image_sources(props: &MaterialsProps) -> impl Iterator<Item = &ImageSource> {
	props
		.layers
		.iter()
		.flat_map(|layer| image_arguments(&layer.args))
		.filter_map(|image| match image {
			ImageRef::Source(source) => Some(source),
			ImageRef::Noise => None,
		})
}

#[cfg(test)]
mod tests;
