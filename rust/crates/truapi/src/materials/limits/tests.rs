use parity_scale_codec::Compact;
use truapi::latest::{
	BoxProps, DragAxis, ImageRef, ImageSource, MaterialFailure, MaterialInstance, MaterialsProps,
	Parameter, Region, RendererNode, TouchCondition, TouchProps, TouchTrigger, TrailProps,
};

use super::*;
use crate::materials::touch_nodes;

const OVER: Option<MaterialFailure> = Some(MaterialFailure::LimitExceeded);

fn image(index: usize) -> ImageRef {
	ImageRef::Source(ImageSource::Archive(format!("{index}.png")))
}

fn layer(material: u8, images: impl IntoIterator<Item = ImageRef>) -> MaterialInstance {
	MaterialInstance {
		material: [material; 32],
		args: images.into_iter().map(Parameter::SamplerFloat).collect(),
	}
}

fn layers(count: usize, material: u8) -> Vec<MaterialInstance> {
	(0..count).map(|_| layer(material, [])).collect()
}

fn node(layers: Vec<MaterialInstance>, children: Vec<RendererNode>) -> RendererNode {
	RendererNode::Materials { props: MaterialsProps { layers }, children }
}

fn tree(nodes: Vec<RendererNode>) -> RendererNode {
	RendererNode::Box {
		modifiers: Vec::new(),
		props: BoxProps { content_alignment: None },
		children: nodes,
	}
}

#[test]
fn a_node_draws_with_16_layers_and_falls_back_with_17() {
	let failures = tree_failures(&tree(vec![node(layers(16, 0), vec![])]));
	assert_eq!(failures, [None]);

	let failures = tree_failures(&tree(vec![node(layers(17, 0), vec![])]));
	assert_eq!(failures, [OVER]);
}

/// Each image argument takes a texture slot, the noise image's too.
#[test]
fn a_layer_draws_with_13_image_arguments_and_falls_back_with_14() {
	let thirteen: Vec<_> = (0..12).map(image).chain([ImageRef::Noise]).collect();
	let failures = tree_failures(&tree(vec![node(vec![layer(0, thirteen.clone())], vec![])]));
	assert_eq!(failures, [None]);

	let fourteen = thirteen.into_iter().chain([ImageRef::Noise]);
	let failures = tree_failures(&tree(vec![node(vec![layer(0, fourteen)], vec![])]));
	assert_eq!(failures, [OVER]);
}

/// The node that would make the 33rd layer falls back. It adds nothing, so a
/// later node that brings the tree to exactly 32 still draws, and every host
/// falls back at the same nodes.
#[test]
fn the_node_that_would_pass_32_layers_falls_back_and_a_later_one_that_fits_draws() {
	let nodes = [16, 10, 7, 6].map(|count| node(layers(count, 0), vec![]));

	assert_eq!(tree_failures(&tree(nodes.to_vec())), [None, None, OVER, None]);
}

#[test]
fn the_17th_distinct_material_falls_back_and_a_repeated_one_does_not_count() {
	let mut nodes: Vec<_> =
		(0..17).map(|material| node(vec![layer(material, [])], vec![])).collect();
	nodes.push(node(vec![layer(0, [])], vec![]));

	let mut expected = vec![None; 16];
	expected.extend([OVER, None]);
	assert_eq!(tree_failures(&tree(nodes)), expected);
}

#[test]
fn the_17th_distinct_image_falls_back_and_repeats_and_noise_do_not_count() {
	let nodes = vec![
		node(vec![layer(0, (0..13).map(image)), layer(0, (13..16).map(image))], vec![]),
		node(vec![layer(0, [image(16)])], vec![]),
		node(vec![layer(0, [image(0), ImageRef::Noise])], vec![]),
	];

	assert_eq!(tree_failures(&tree(nodes)), [None, OVER, None]);
}

/// Nodes are numbered depth first, as failure reports number them. A nested
/// node falls back without taking its ancestor with it, and its layers do not
/// count: here they would make the last node the 33rd layer.
#[test]
fn a_nested_node_falls_back_alone_and_comes_before_the_next_sibling() {
	let nested = node(layers(1, 0), vec![]);
	let nodes = vec![node(layers(16, 0), vec![nested]), node(layers(16, 0), vec![])];

	assert_eq!(tree_failures(&tree(nodes)), [None, OVER, None]);
}

/// Shaderlab refuses a stack without layers, so hosts report it too.
#[test]
fn a_node_without_layers_is_an_invalid_stack() {
	assert_eq!(
		tree_failures(&tree(vec![node(Vec::new(), vec![])])),
		[Some(MaterialFailure::InvalidStack)]
	);
}

/// A node that breaks the image total adds none of its materials either, or
/// adding totals one at a time would pass the edge tests above.
#[test]
fn a_node_over_one_total_adds_nothing_to_the_others() {
	let mut nodes = vec![
		node(vec![layer(0, (0..13).map(image)), layer(0, (13..16).map(image))], vec![]),
		node(vec![layer(99, [image(16)])], vec![]),
	];
	nodes.extend((1..16).map(|material| node(vec![layer(material, [])], vec![])));

	let mut expected = vec![None, OVER];
	expected.extend([None; 15]);
	assert_eq!(tree_failures(&tree(nodes)), expected);
}

const INVALID: Option<MaterialFailure> = Some(MaterialFailure::InvalidStack);

fn touch_layer(args: Vec<Parameter>) -> RendererNode {
	node(vec![MaterialInstance { material: [0; 32], args }], vec![])
}

fn touch(trail: bool, triggers: Vec<TouchCondition>, children: Vec<RendererNode>) -> RendererNode {
	RendererNode::Touch {
		props: TouchProps {
			drag: DragAxis::Any,
			trail: trail.then_some(TrailProps { radius: Compact(20) }),
			triggers: triggers
				.into_iter()
				.map(|when| TouchTrigger { when, action: "revealed".to_string() })
				.collect(),
		},
		children,
	}
}

/// A touch argument outside a `Touch` node has no finger to read, so every
/// host draws its node plain. The clock needs no `Touch` node.
#[test]
fn a_touch_argument_needs_a_touch_node_around_it() {
	let finger = || touch_layer(vec![Parameter::Touch, Parameter::TouchState]);
	let clock = touch_layer(vec![Parameter::Elapsed]);

	assert_eq!(tree_failures(&tree(vec![finger(), clock])), [INVALID, None]);
	assert_eq!(tree_failures(&touch(false, Vec::new(), vec![finger()])), [None]);
}

#[test]
fn the_trail_needs_a_touch_node_that_keeps_one() {
	let trail = || touch_layer(vec![Parameter::SamplerFloat(ImageRef::Trail)]);

	assert_eq!(tree_failures(&touch(false, Vec::new(), vec![trail()])), [INVALID]);
	assert_eq!(tree_failures(&touch(true, Vec::new(), vec![trail()])), [None]);
}

/// The fifth `Touch` node, a nested one, and one breaking its own limits draw
/// like a `Box`, so the materials inside read no finger.
#[test]
fn a_touch_node_past_a_limit_takes_no_touch() {
	let finger = || touch_layer(vec![Parameter::Touch]);
	let five: Vec<_> = (0..5).map(|_| touch(false, Vec::new(), vec![finger()])).collect();
	let five = tree(five);
	assert_eq!(touch_nodes(&five), [true, true, true, true, false]);
	assert_eq!(tree_failures(&five), [None, None, None, None, INVALID]);

	let nested = touch(false, Vec::new(), vec![touch(false, Vec::new(), vec![finger()])]);
	assert_eq!(touch_nodes(&nested), [true, false]);
	assert_eq!(tree_failures(&nested), [INVALID]);

	let region = Region { x: 900, y: 0, width: 101, height: 1000 };
	let invalid = [
		touch(false, vec![TouchCondition::TrailCovers(600)], vec![finger()]),
		touch(false, vec![TouchCondition::Tap(region)], vec![finger()]),
		touch(true, vec![TouchCondition::TrailCovers(600); 9], vec![finger()]),
	];
	for node in invalid {
		assert_eq!((touch_nodes(&node), tree_failures(&node)), (vec![false], vec![INVALID]));
	}
	let eight = touch(true, vec![TouchCondition::TrailCovers(600); 8], vec![finger()]);
	assert_eq!(touch_nodes(&eight), [true]);
}
