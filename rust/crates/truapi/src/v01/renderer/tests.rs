use super::*;

#[derive(Encode)]
struct RendererWireComponent<P> {
	modifiers: Vec<Modifier>,
	props: P,
	children: Vec<RendererNode>,
}

#[derive(Encode)]
enum RendererWireNode<P> {
	#[codec(index = 3)]
	Column(RendererWireComponent<P>),
}

fn renderer_node() -> RendererNode {
	RendererNode::Column {
		modifiers: vec![Modifier::Padding(Dimensions {
			top: Compact(12),
			end: Compact(8),
			bottom: None,
			start: Some(Compact(4)),
		})],
		props: ColumnProps {
			horizontal_alignment: Some(HorizontalAlignment::Center),
			vertical_arrangement: Some(Arrangement::SpaceBetween),
		},
		children: vec![
			RendererNode::String { text: "Votes: 1".to_string() },
			RendererNode::Button {
				modifiers: Vec::new(),
				props: ButtonProps {
					text: "Vote".to_string(),
					variant: Some(ButtonVariant::Primary),
					enabled: OptionBool(Some(true)),
					loading: OptionBool(None),
					click_action: Some("vote".to_string()),
				},
				children: Vec::new(),
			},
			RendererNode::Spacer { modifiers: vec![Modifier::Opacity(128)] },
			RendererNode::Image {
				modifiers: vec![Modifier::Width(Compact(40))],
				props: ImageProps {
					source: ImageSource::Bulletin("bafy".to_string()),
					fit: Some(ImageFit::Cover),
				},
			},
			RendererNode::Image {
				modifiers: Vec::new(),
				props: ImageProps { source: ImageSource::Inline(vec![0x89, 0x50]), fit: None },
			},
			RendererNode::Materials {
				props: MaterialsProps {
					layers: vec![MaterialInstance {
						material: [7; 32],
						args: vec![
							Parameter::Float(1.5f32.to_bits()),
							Parameter::Vec2([1, 2]),
							Parameter::Vec3([1, 2, 3]),
							Parameter::Vec4([1, 2, 3, 4]),
							Parameter::SamplerFloat(ImageRef::Noise),
							Parameter::SamplerVec4(ImageRef::Source(ImageSource::Archive(
								"mask.png".to_string(),
							))),
						],
					}],
					bleed: Compact(0),
				},
				children: vec![RendererNode::Nil],
			},
			touch_node(),
		],
	}
}

fn touch_node() -> RendererNode {
	RendererNode::Touch {
		props: TouchProps {
			drag: DragAxis::Horizontal,
			trail: Some(TrailProps { radius: Compact(20) }),
			triggers: vec![
				TouchTrigger {
					when: TouchCondition::Drag {
						from: Region { x: 0, y: 0, width: 200, height: 1000 },
						to: Region { x: 900, y: 0, width: 100, height: 1000 },
					},
					action: "confirm".to_string(),
				},
				TouchTrigger {
					when: TouchCondition::TrailCovers(600),
					action: "revealed".to_string(),
				},
			],
			replay: Some(ReplayProps { length: 2200 }),
		},
		children: vec![RendererNode::Materials {
			props: MaterialsProps {
				layers: vec![MaterialInstance {
					material: [9; 32],
					args: vec![
						Parameter::Touch,
						Parameter::TouchState,
						Parameter::Elapsed,
						Parameter::Replay,
						Parameter::SamplerFloat(ImageRef::Trail),
					],
				}],
				bleed: Compact(48),
			},
			children: Vec::new(),
		}],
	}
}

#[test]
fn column_preserves_the_component_wire_shape() {
	let node = renderer_node();
	let RendererNode::Column { modifiers, props, children } = node.clone() else {
		unreachable!();
	};
	let wire = RendererWireNode::Column(RendererWireComponent { modifiers, props, children });

	assert_eq!(node.encode(), wire.encode());
}

#[test]
fn variants_encode_only_the_fields_they_carry() {
	let spacer = RendererNode::Spacer { modifiers: Vec::new() };
	assert_eq!(spacer.encode(), vec![5, 0]);

	let materials = RendererNode::Materials {
		props: MaterialsProps { layers: Vec::new(), bleed: Compact(0) },
		children: Vec::new(),
	};
	assert_eq!(materials.encode(), vec![11, 0, 0, 0]);

	let touch = RendererNode::Touch {
		props: TouchProps { drag: DragAxis::Any, trail: None, triggers: Vec::new(), replay: None },
		children: Vec::new(),
	};
	assert_eq!(touch.encode(), vec![13, 1, 0, 0, 0, 0]);

	assert_eq!(ImageSource::Inline(vec![1]).encode(), vec![2, 4, 1]);
}

/// The medium weight comes after the older presets, so a tree that names one
/// of those decodes as before.
#[test]
fn older_typography_presets_keep_their_indices() {
	let encoded: Vec<Vec<u8>> =
		[TypographyStyle::BodySmallRegular, TypographyStyle::BodyMediumMedium]
			.iter()
			.map(Encode::encode)
			.collect();

	assert_eq!(encoded, [vec![4], vec![5]]);
}

/// Index 10 stays unused, so an older tree is refused rather than misread.
#[test]
fn index_10_is_never_read() {
	assert!(RendererNode::decode(&mut [10u8, 0, 0].as_slice()).is_err());
}

/// Index 12 is left to the `SlideToConfirm` node of an open pull request, so
/// the two can land in either order.
#[test]
fn index_12_is_not_read() {
	assert!(RendererNode::decode(&mut [12u8, 0, 0].as_slice()).is_err());
}

/// The touch arguments come after every older one, so a tree encoded before
/// them decodes to the same value.
#[test]
fn older_arguments_keep_their_indices() {
	let encoded: Vec<Vec<u8>> = [
		Parameter::SamplerVec4(ImageRef::Noise),
		Parameter::Touch,
		Parameter::TouchState,
		Parameter::Elapsed,
		Parameter::Replay,
		Parameter::SamplerFloat(ImageRef::Trail),
	]
	.iter()
	.map(Encode::encode)
	.collect();

	assert_eq!(encoded, [vec![7, 1], vec![8], vec![9], vec![10], vec![11], vec![4, 2]]);
}

#[test]
fn a_touch_tree_reads_from_json() {
	let json = r#"{"tag":"Touch","value":{"props":{"drag":"Horizontal","trail":{"radius":20},
		"triggers":[{"when":{"tag":"Drag","value":{"from":{"x":0,"y":0,"width":200,"height":1000},
		"to":{"x":900,"y":0,"width":100,"height":1000}}},"action":"confirm"},
		{"when":{"tag":"TrailCovers","value":600},"action":"revealed"}],"replay":{"length":2200}},
		"children":[{"tag":"Materials","value":{"props":{"layers":[{"material":"0x0909090909090909090909090909090909090909090909090909090909090909",
		"args":[{"tag":"Touch"},{"tag":"TouchState"},{"tag":"Elapsed"},{"tag":"Replay"},{"tag":"SamplerFloat","value":{"tag":"Trail"}}]}],"bleed":48},
		"children":[]}}]}}"#;

	assert_eq!(serde_json::from_str::<RendererNode>(json).unwrap(), touch_node());
}

#[test]
fn tree_round_trips_through_scale() {
	let node = renderer_node();
	let decoded = RendererNode::decode(&mut node.encode().as_slice()).unwrap();
	assert_eq!(decoded, node);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn tree_round_trips_through_uniffi() {
	let node = renderer_node();
	let ffi = <RendererNode as uniffi::Lower<crate::UniFfiTag>>::lower(node.clone());
	let lifted = <RendererNode as uniffi::Lift<crate::UniFfiTag>>::try_lift(ffi).unwrap();

	assert_eq!(lifted, node);
}

fn inline_image(bytes: Vec<u8>) -> RendererNode {
	RendererNode::Image {
		modifiers: vec![Modifier::Width(Compact(40))],
		props: ImageProps { source: ImageSource::Inline(bytes), fit: None },
	}
}

/// Every host refuses the same inline images, so a body draws alike
/// everywhere: raster formats recognised by their first bytes, nothing over
/// 64 KiB, and never SVG, which can load outside addresses.
#[test]
fn only_small_raster_inline_images_keep_their_bytes() {
	let png = b"\x89PNG\r\n\x1a\n".to_vec();
	let padded = |mut bytes: Vec<u8>, len: usize| {
		bytes.resize(len, 0);
		bytes
	};
	let cases = [
		(png.clone(), true),
		(vec![0xff, 0xd8, 0xff, 0xe0], true),
		(b"RIFF\0\0\0\0WEBPVP8 ".to_vec(), true),
		(padded(png.clone(), 64 * 1024), true),
		(padded(png, 64 * 1024 + 1), false),
		(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_vec(), false),
		(b"not an image".to_vec(), false),
	];

	for (bytes, allowed) in cases {
		let mut node = inline_image(bytes.clone());
		node.clear_refused_inline_images();
		let expected = inline_image(if allowed { bytes } else { Vec::new() });
		assert_eq!(node, expected, "allowed: {allowed}");
	}
}

/// A refused image keeps its node and modifiers, so it still takes its
/// place as empty space, wherever in the tree it sits, sampler arguments
/// included.
#[test]
fn refused_inline_images_are_emptied_throughout_the_tree() {
	let svg = b"<svg/>".to_vec();
	let tree = |image: Vec<u8>| RendererNode::Column {
		modifiers: Vec::new(),
		props: ColumnProps { horizontal_alignment: None, vertical_arrangement: None },
		children: vec![RendererNode::Materials {
			props: MaterialsProps {
				layers: vec![MaterialInstance {
					material: [7; 32],
					args: vec![Parameter::SamplerVec4(ImageRef::Source(ImageSource::Inline(
						image.clone(),
					)))],
				}],
				bleed: Compact(0),
			},
			children: vec![inline_image(image)],
		}],
	};

	let mut node = tree(svg);
	node.clear_refused_inline_images();

	assert_eq!(node, tree(Vec::new()));
}
