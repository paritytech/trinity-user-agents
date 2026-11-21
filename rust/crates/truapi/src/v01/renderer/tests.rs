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
				},
				children: vec![RendererNode::Nil],
			},
		],
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
		props: MaterialsProps { layers: Vec::new() },
		children: Vec::new(),
	};
	assert_eq!(materials.encode(), vec![11, 0, 0]);

	assert_eq!(ImageSource::Inline(vec![1]).encode(), vec![2, 4, 1]);
}

fn slide_to_confirm() -> RendererNode {
	RendererNode::SlideToConfirm {
		modifiers: Vec::new(),
		props: SlideToConfirmProps {
			text: "Go".to_string(),
			secondary_text: None,
			confirmed_text: None,
			variant: Some(SlideToConfirmVariant::Reveal),
			accent: None,
			enabled: OptionBool(None),
			loading: OptionBool(None),
			confirmed: OptionBool(Some(true)),
			confirm_action: Some("r".to_string()),
		},
	}
}

/// Index 11 belongs to `Materials`, so a host that reads index 12 as anything
/// else would misread the control instead of refusing it.
#[test]
fn slide_to_confirm_is_variant_twelve() {
	let expected = [
		// Variant 12, then an empty modifier list.
		vec![12, 0],
		"Go".to_string().encode(),
		// No second line, no confirmed text, `Some(Reveal)`, no accent.
		vec![0, 0, 1, 1, 0],
		// `OptionBool` is one byte: enabled and loading absent, confirmed true.
		vec![0, 0, 1],
		Some("r".to_string()).encode(),
	]
	.concat();

	assert_eq!(slide_to_confirm().encode(), expected);
}

#[test]
fn slide_to_confirm_round_trips_through_scale() {
	let node = slide_to_confirm();
	let decoded = RendererNode::decode(&mut node.encode().as_slice()).unwrap();
	assert_eq!(decoded, node);
}

/// Index 10 stays unused, so an older tree is refused rather than misread.
#[test]
fn index_10_is_never_read() {
	assert!(RendererNode::decode(&mut [10u8, 0, 0].as_slice()).is_err());
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
			},
			children: vec![inline_image(image)],
		}],
	};

	let mut node = tree(svg);
	node.clear_refused_inline_images();

	assert_eq!(node, tree(Vec::new()));
}
