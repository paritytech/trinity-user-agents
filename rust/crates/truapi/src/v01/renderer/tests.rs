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

#[test]
fn gesture_keeps_its_wire_index_and_children() {
	let node = RendererNode::Gesture {
		modifiers: Vec::new(),
		props: GestureProps {
			tap_action: None,
			pan: None,
			accessibility_label: None,
			enabled: OptionBool(None),
			haptic_on_complete: OptionBool(None),
			key: None,
			feel: None,
		},
		children: vec![RendererNode::Nil],
	};
	assert_eq!(node.encode(), vec![12, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0]);
	assert_eq!(RendererNode::decode(&mut node.encode().as_slice()).unwrap(), node);
}

#[test]
fn gesture_modes_round_trip_with_actions_and_motion() {
	for drag in [
		None,
		Some(PanDrag {
			travel: PanTravel::ParentWidth,
			release: PanRelease::Complete { threshold_percent: 90 },
			hold_at_end: OptionBool(Some(true)),
		}),
		Some(PanDrag {
			travel: PanTravel::Points(Compact(240)),
			release: PanRelease::SnapBack,
			hold_at_end: OptionBool(Some(false)),
		}),
	] {
		let node = RendererNode::Gesture {
			modifiers: vec![Modifier::FillWidth(true)],
			props: GestureProps {
				tap_action: Some("tap".into()),
				pan: Some(PanGesture {
					axis: PanAxis::Horizontal,
					drag,
					start_action: Some("start".into()),
					move_action: Some("move".into()),
					move_interval_ms: Some(0),
					end_action: Some("end".into()),
					location: OptionBool(None),
				}),
				accessibility_label: Some("Register".into()),
				enabled: OptionBool(Some(true)),
				haptic_on_complete: OptionBool(Some(true)),
				key: Some("piece".into()),
				feel: None,
			},
			children: vec![renderer_node()],
		};
		assert_eq!(RendererNode::decode(&mut node.encode().as_slice()).unwrap(), node);
	}
}

#[test]
fn gesture_payload_has_the_same_bytes_on_hosts_and_clients() {
	let event =
		GestureEvent { phase: GesturePhase::Completed, x: 240, y: 0, fraction_permille: 1000 };
	let expected = [5, 240, 0, 0, 0, 0, 0, 0, 0, 232, 3];
	assert_eq!(event.encode(), expected);
	assert_eq!(GestureEvent::decode(&mut expected.as_slice()).unwrap(), event);
}

#[test]
fn set_offset_has_the_same_bytes_on_hosts_and_clients() {
	let node = RendererNode::SetOffset { key: "piece".into(), x: 120, y: -2 };
	let expected = [13, 20, b'p', b'i', b'e', b'c', b'e', 120, 0, 0, 0, 254, 255, 255, 255];
	assert_eq!(node.encode(), expected);
	assert_eq!(RendererNode::decode(&mut expected.as_slice()).unwrap(), node);
}

/// An offset update is eleven bytes plus its key, against hundreds for the
/// tree it moves a part of.
#[test]
fn set_offset_is_smaller_than_any_tree_holding_its_gesture() {
	let offset = RendererNode::SetOffset { key: "piece".into(), x: 120, y: 0 };
	let tree = RendererNode::Gesture {
		modifiers: Vec::new(),
		props: GestureProps {
			tap_action: None,
			pan: None,
			accessibility_label: None,
			enabled: OptionBool(None),
			haptic_on_complete: OptionBool(None),
			key: Some("piece".into()),
			feel: None,
		},
		children: vec![renderer_node()],
	};
	assert_eq!(offset.encode().len(), 15);
	assert!(offset.encode().len() < tree.encode().len());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn set_offset_and_keyed_gesture_round_trip_through_uniffi() {
	for node in [
		RendererNode::SetOffset { key: "piece".into(), x: -7, y: 3 },
		RendererNode::Gesture {
			modifiers: Vec::new(),
			props: GestureProps {
				tap_action: None,
				pan: None,
				accessibility_label: None,
				enabled: OptionBool(None),
				haptic_on_complete: OptionBool(None),
				key: Some("piece".into()),
				feel: None,
			},
			children: vec![RendererNode::Nil],
		},
	] {
		let ffi = <RendererNode as uniffi::Lower<crate::UniFfiTag>>::lower(node.clone());
		let lifted = <RendererNode as uniffi::Lift<crate::UniFfiTag>>::try_lift(ffi).unwrap();
		assert_eq!(lifted, node);
	}
}

/// Every modifier that styles by value.
fn styled_by_value() -> Vec<Modifier> {
	vec![
		Modifier::Fill(Fill { color: 0x3336_3AFF, shape: Some(Shape::Rounded(Compact(40))) }),
		Modifier::Shadow(DropShadow { color: 0x0000_005C, x: 0, y: 200, blur: 600 }),
		Modifier::Translate(Translate { x: 0, y: -180 }),
		Modifier::Font(TextFont {
			size: 1800,
			line_height: 2200,
			weight: 600,
			color: 0xF5F6_F9FF,
			letter_spacing: 0,
		}),
		Modifier::FadeWithPan(PanFade { key: "piece".into(), rate_permille: 1400 }),
		Modifier::Shimmer(Shimmer {
			color: 0xFFFF_FFFF,
			stops: vec![0, 3, 14, 46, 112, 195, 247, 195, 112, 46, 14, 3, 0],
			band: 18_600,
			lead: 7000,
			speed: 21_200,
			rest_ms: 1000,
		}),
		Modifier::Spin(800),
		Modifier::ExitFade(350),
	]
}

#[test]
fn modifiers_by_value_follow_blending_mode_on_the_wire() {
	// Indices 0 to 11 are the modifiers before these; each new one is appended.
	let first = [12u8, 13, 14, 15, 16, 17, 18, 19];
	for (modifier, index) in styled_by_value().into_iter().zip(first) {
		assert_eq!(modifier.encode()[0], index, "{modifier:?}");
	}
	assert_eq!(
		Modifier::Fill(Fill { color: 0x3336_3AFF, shape: None }).encode(),
		vec![12, 0xFF, 0x3A, 0x36, 0x33, 0]
	);
	assert_eq!(Modifier::ExitFade(350).encode(), vec![19, 0x5E, 0x01]);
}

#[test]
fn modifiers_by_value_round_trip_through_scale() {
	let node = RendererNode::Box {
		modifiers: styled_by_value(),
		props: BoxProps { content_alignment: Some(ContentAlignment::CenterStart) },
		children: vec![renderer_node()],
	};
	assert_eq!(RendererNode::decode(&mut node.encode().as_slice()).unwrap(), node);
}

const fn point(x: i32, y: i32) -> MaskPoint {
	MaskPoint { x, y }
}

fn mask(shapes: Vec<MaskShape>, follow: Option<MaskFollow>, fade_ms: Option<u16>) -> Mask {
	Mask {
		key: Some("mask".into()),
		mode: MaskMode::Hide,
		shapes,
		follow,
		fade_ms,
		cleared_share: None,
		haptics: None,
	}
}

/// A location pan over a picture that carries a keyed mask.
fn masked_tree(mask: Mask) -> RendererNode {
	RendererNode::Gesture {
		modifiers: vec![Modifier::Width(Compact(280)), Modifier::Height(Compact(300))],
		props: GestureProps {
			tap_action: None,
			pan: Some(PanGesture {
				axis: PanAxis::Free,
				drag: None,
				start_action: Some("pan.start".into()),
				move_action: Some("pan.move".into()),
				move_interval_ms: None,
				end_action: Some("pan.end".into()),
				location: OptionBool(Some(true)),
			}),
			accessibility_label: None,
			enabled: OptionBool(None),
			haptic_on_complete: OptionBool(None),
			key: Some("pan".into()),
			feel: None,
		},
		children: vec![RendererNode::Box {
			modifiers: vec![Modifier::FillWidth(true), Modifier::Mask(mask)],
			props: BoxProps { content_alignment: None },
			children: vec![renderer_node()],
		}],
	}
}

fn every_shape() -> Vec<MaskShape> {
	vec![
		MaskShape::Circle { center: point(1200, -300), radius: 2600 },
		MaskShape::Stroke {
			width: 5200,
			points: vec![point(0, 0), point(1050, 2075), point(-5, 30000)],
		},
		MaskShape::Stroke { width: 300, points: vec![point(4, 4)] },
		MaskShape::Stamped {
			width: 5200,
			brush: slanted_nib(),
			points: vec![point(0, 0), point(2000, 900)],
		},
		MaskShape::All,
	]
}

/// A narrow ellipse turned 30 degrees anticlockwise, stamped every quarter of
/// its length.
fn slanted_nib() -> MaskBrush {
	MaskBrush {
		tip: MaskBrushTip::Ellipse,
		aspect_permille: 350,
		angle_centideg: -3000,
		spacing_permille: 250,
	}
}

#[test]
fn a_masked_node_round_trips_with_every_shape_follow_fade_and_share() {
	for mask in [
		mask(Vec::new(), None, None),
		mask(
			every_shape(),
			Some(MaskFollow { pan: "pan".into(), width: 5200, brush: None }),
			Some(350),
		),
		Mask {
			key: None,
			mode: MaskMode::Show,
			shapes: every_shape(),
			follow: None,
			fade_ms: Some(0),
			cleared_share: Some(ClearedShare {
				at_permille: 550,
				action: "cleared".into(),
				while_down: OptionBool(None),
				rescue: None,
			}),
			haptics: None,
		},
	] {
		let node = masked_tree(mask);
		assert_eq!(RendererNode::decode(&mut node.encode().as_slice()).unwrap(), node);
	}
}

/// The mask is the modifier after `ExitFade`; an empty unkeyed one is a byte
/// per field.
#[test]
fn a_mask_is_the_modifier_after_exit_fade() {
	let empty = Modifier::Mask(Mask {
		key: None,
		mode: MaskMode::Hide,
		shapes: Vec::new(),
		follow: None,
		fade_ms: None,
		cleared_share: None,
		haptics: None,
	});
	assert_eq!(empty.encode(), vec![20, 0, 0, 0, 0, 0, 0, 0]);
	let show = Modifier::Mask(Mask {
		key: Some("k".into()),
		mode: MaskMode::Show,
		shapes: vec![MaskShape::All],
		follow: Some(MaskFollow { pan: "p".into(), width: 100, brush: None }),
		fade_ms: Some(350),
		cleared_share: Some(ClearedShare {
			at_permille: 550,
			action: "a".into(),
			while_down: OptionBool(None),
			rescue: None,
		}),
		haptics: None,
	});
	// The share follows the fade: present, 550 as a little-endian u16, the action.
	assert_eq!(
		show.encode(),
		vec![
			20, 1, 4, b'k', 1, 4, 0, 1, 4, b'p', 100, 0, 0, 0, 0, 1, 94, 1, 1, 38, 2, 4, b'a', 0,
			0, 0
		]
	);
	assert_eq!(Modifier::decode(&mut show.encode().as_slice()).unwrap(), show);
}

/// The box the client's wire-contract test encodes, so both sides pin one
/// vector: the box's index, one modifier, the mask, no alignment, no children.
#[test]
fn a_masked_box_has_the_bytes_the_client_pins() {
	let node = RendererNode::Box {
		modifiers: vec![Modifier::Mask(Mask {
			key: Some("k".into()),
			mode: MaskMode::Show,
			shapes: vec![MaskShape::All],
			follow: Some(MaskFollow { pan: "p".into(), width: 100, brush: None }),
			fade_ms: Some(350),
			cleared_share: Some(ClearedShare {
				at_permille: 550,
				action: "a".into(),
				while_down: OptionBool(None),
				rescue: None,
			}),
			haptics: None,
		})],
		props: BoxProps { content_alignment: None },
		children: Vec::new(),
	};
	assert_eq!(
		node.encode(),
		vec![
			2, 4, 20, 1, 4, b'k', 1, 4, 0, 1, 4, b'p', 100, 0, 0, 0, 0, 1, 94, 1, 1, 38, 2, 4,
			b'a', 0, 0, 0, 0, 0
		]
	);
}

#[test]
fn extend_mask_has_the_same_bytes_on_hosts_and_clients() {
	let node = RendererNode::ExtendMask {
		key: "mask".into(),
		shapes: vec![
			MaskShape::Circle { center: point(100, -1), radius: 2600 },
			MaskShape::Stroke { width: 5200, points: vec![point(1, 2)] },
		],
	};
	// Index 14, the key, a compact count of two shapes: a circle (centre as
	// two little-endian i32, radius as a u32), then a stroke (width, a compact
	// count of one point).
	let expected = [
		14, 16, b'm', b'a', b's', b'k', 8, //
		1, 100, 0, 0, 0, 255, 255, 255, 255, 40, 10, 0, 0, //
		2, 80, 20, 0, 0, 4, 1, 0, 0, 0, 2, 0, 0, 0,
	];
	assert_eq!(node.encode(), expected);
	assert_eq!(RendererNode::decode(&mut expected.as_slice()).unwrap(), node);
}

/// With `follow` the host draws the stroke itself, so a move's answer is an
/// empty extend: the key and an empty list. Sending the segment instead is
/// still a few dozen bytes, and either stays far smaller than the tree.
#[test]
fn answering_a_move_costs_a_few_bytes() {
	let empty = RendererNode::ExtendMask { key: "mask.top".into(), shapes: Vec::new() };
	assert_eq!(empty.encode().len(), 11);
	let segment = RendererNode::ExtendMask {
		key: "mask.top".into(),
		shapes: vec![MaskShape::Stroke {
			width: 5200,
			points: vec![point(0, 0), point(900, 1300)],
		}],
	};
	assert_eq!(segment.encode().len(), 33);
	let stroke = MaskShape::Stroke { width: 5200, points: (0..400).map(|i| point(i, i)).collect() };
	let tree = masked_tree(mask(vec![stroke], None, None));
	assert!(segment.encode().len() * 50 < tree.encode().len());
}

#[test]
fn a_location_pan_is_a_flag_after_the_end_action() {
	let pan = |location| PanGesture {
		axis: PanAxis::Free,
		drag: None,
		start_action: None,
		move_action: None,
		move_interval_ms: None,
		end_action: None,
		location: OptionBool(location),
	};
	assert_eq!(pan(None).encode(), vec![2, 0, 0, 0, 0, 0, 0]);
	assert_eq!(pan(Some(true)).encode(), vec![2, 0, 0, 0, 0, 0, 1]);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn modifiers_by_value_round_trip_through_uniffi() {
	for modifier in styled_by_value() {
		let ffi = <Modifier as uniffi::Lower<crate::UniFfiTag>>::lower(modifier.clone());
		let lifted = <Modifier as uniffi::Lift<crate::UniFfiTag>>::try_lift(ffi).unwrap();
		assert_eq!(lifted, modifier);
	}
}

#[test]
fn modifiers_by_value_read_from_json() {
	let json = r#"[
		{"tag":"Fill","value":{"color":859192063,"shape":{"tag":"Circle"}}},
		{"tag":"Shadow","value":{"color":92,"x":0,"y":200,"blur":600}},
		{"tag":"Translate","value":{"x":0,"y":-180}},
		{"tag":"Font","value":{"size":1800,"lineHeight":2200,"weight":400,"color":4294967295,"letterSpacing":0}},
		{"tag":"FadeWithPan","value":{"key":"piece","ratePermille":1400}},
		{"tag":"Shimmer","value":{"color":4294967295,"stops":"0x00ff00","band":18600,"lead":7000,"speed":21200,"restMs":1000}},
		{"tag":"Spin","value":800},
		{"tag":"ExitFade","value":350}
	]"#;
	let modifiers: Vec<Modifier> = serde_json::from_str(json).unwrap();
	assert_eq!(modifiers.len(), 8);
	assert_eq!(
		modifiers[0],
		Modifier::Fill(Fill { color: 0x3336_3AFF, shape: Some(Shape::Circle) })
	);
	assert_eq!(
		modifiers[3],
		Modifier::Font(TextFont {
			size: 1800,
			line_height: 2200,
			weight: 400,
			color: 0xFFFF_FFFF,
			letter_spacing: 0,
		})
	);
	assert_eq!(modifiers[7], Modifier::ExitFade(350));
}

/// A feel with every field set.
fn full_feel() -> PanFeel {
	PanFeel {
		press_scale_permille: 1059,
		press_ms: 133,
		unpress_ms: 150,
		spring_omega_centi: 1800,
		spring_damping_centi: 80,
		approach_ms: 117,
		approach_reference: 4200,
		pop_permille: vec![1059, 1132, 1192, 1237, 1000],
		pop_offset: 333,
		press_scale_y_permille: None,
	}
}

#[test]
fn a_gesture_feel_round_trips_through_scale_json_and_uniffi() {
	let node = RendererNode::Gesture {
		modifiers: Vec::new(),
		props: GestureProps {
			tap_action: None,
			pan: None,
			accessibility_label: None,
			enabled: OptionBool(None),
			haptic_on_complete: OptionBool(None),
			key: Some("piece".into()),
			feel: Some(full_feel()),
		},
		children: vec![RendererNode::Nil],
	};
	assert_eq!(RendererNode::decode(&mut node.encode().as_slice()).unwrap(), node);
	let json = r#"{"pressScalePermille":1059,"pressMs":133,"unpressMs":150,"springOmegaCenti":1800,
		"springDampingCenti":80,"approachMs":117,"approachReference":4200,"popPermille":[1059,1132,1192,1237,1000],"popOffset":333}"#;
	assert_eq!(serde_json::from_str::<PanFeel>(json).unwrap(), full_feel());
	#[cfg(not(target_arch = "wasm32"))]
	{
		let ffi = <RendererNode as uniffi::Lower<crate::UniFfiTag>>::lower(node.clone());
		assert_eq!(<RendererNode as uniffi::Lift<crate::UniFfiTag>>::try_lift(ffi).unwrap(), node);
	}
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn masks_and_extends_round_trip_through_uniffi() {
	for node in [
		masked_tree(mask(
			every_shape(),
			Some(MaskFollow { pan: "pan".into(), width: 5200, brush: None }),
			Some(350),
		)),
		RendererNode::ExtendMask { key: "mask".into(), shapes: every_shape() },
		RendererNode::ExtendMask { key: "mask".into(), shapes: Vec::new() },
	] {
		let ffi = <RendererNode as uniffi::Lower<crate::UniFfiTag>>::lower(node.clone());
		let lifted = <RendererNode as uniffi::Lift<crate::UniFfiTag>>::try_lift(ffi).unwrap();
		assert_eq!(lifted, node);
	}
}

/// A 10 by 6 logical pixel node is 5 by 3 samples, at 1, 3, 5, 7, 9 by 1, 3, 5.
const NODE: (u32, u32) = (1000, 600);

#[test]
fn the_cleared_share_samples_two_pixel_squares_from_the_top_start_corner() {
	assert_eq!(cleared_share(&[], NODE.0, NODE.1), (0, 15));
	assert_eq!(cleared_share(&[MaskShape::All], NODE.0, NODE.1), (15, 15));
	// A part square counts as a whole sample: 1001 wide is 6 columns.
	assert_eq!(cleared_share(&[], 1001, 600), (0, 18));
	assert_eq!(cleared_share(&[], 0, 600), (0, 0));
}

#[test]
fn a_shape_covers_the_samples_on_its_edge_and_not_one_past_it() {
	// A circle at the first sample, its radius exactly the 200 to the next.
	let circle = |radius| [MaskShape::Circle { center: point(100, 100), radius }];
	assert_eq!(cleared_share(&circle(200), NODE.0, NODE.1), (3, 15));
	assert_eq!(cleared_share(&circle(199), NODE.0, NODE.1), (1, 15));
	// A stroke along the middle row, 400 wide: its edges pass through the
	// rows above and below.
	let row = |width| [MaskShape::Stroke { width, points: vec![point(100, 300), point(900, 300)] }];
	assert_eq!(cleared_share(&row(400), NODE.0, NODE.1), (15, 15));
	assert_eq!(cleared_share(&row(398), NODE.0, NODE.1), (5, 15));
	// Round caps: a dot is a circle half its width across.
	let dot = [MaskShape::Stroke { width: 400, points: vec![point(500, 300)] }];
	assert_eq!(cleared_share(&dot, NODE.0, NODE.1), (5, 15));
	// A shape reaching outside the node counts only the samples inside it.
	let outside = [MaskShape::Circle { center: point(-100, -100), radius: 300 }];
	assert_eq!(cleared_share(&outside, NODE.0, NODE.1), (1, 15));
	// Overlapping shapes count a sample once.
	let twice = [circle(200)[0].clone(), circle(200)[0].clone()];
	assert_eq!(cleared_share(&twice, NODE.0, NODE.1), (3, 15));
}

#[test]
fn a_cleared_share_is_reached_exactly_at_its_line() {
	let share = |at_permille| ClearedShare {
		at_permille,
		action: "a".into(),
		while_down: OptionBool(None),
		rescue: None,
	};
	assert!(share(550).reached(713, 1295));
	assert!(!share(550).reached(712, 1295));
	assert!(share(500).reached(2, 4));
	assert!(!share(500).reached(1, 4));
	assert!(share(0).reached(0, 4));
	assert!(!share(1001).reached(4, 4));
	assert!(!share(u16::MAX).reached(u32::MAX, u32::MAX));
}

/// A node that takes no modifiers, such as `Materials`, is masked through a
/// `Box` that carries the mask; the wrapped node's own bytes do not change.
#[test]
fn a_mask_wraps_any_node_and_leaves_its_bytes_unchanged() {
	let materials = RendererNode::Materials {
		props: MaterialsProps {
			layers: vec![MaterialInstance { material: [7; 32], args: vec![Parameter::Float(0)] }],
		},
		children: vec![RendererNode::Nil],
	};
	for child in [materials, renderer_node(), masked_tree(mask(Vec::new(), None, None))] {
		let wrapped = RendererNode::Box {
			modifiers: vec![Modifier::Mask(mask(every_shape(), None, Some(350)))],
			props: BoxProps { content_alignment: None },
			children: vec![child.clone()],
		};
		let bytes = wrapped.encode();
		assert!(bytes.ends_with(&child.encode()), "{child:?}");
		assert_eq!(RendererNode::decode(&mut bytes.as_slice()).unwrap(), wrapped);
	}
}

/// A stamped line is the shape after `Stroke`: its width, then the brush's
/// tip, aspect, angle and spacing, then its points. A followed brush is the
/// field after the width.
#[test]
fn a_stamped_line_and_a_followed_brush_have_the_same_bytes_on_hosts_and_clients() {
	let stamped = MaskShape::Stamped {
		width: 5200,
		brush: MaskBrush {
			tip: MaskBrushTip::Capsule,
			aspect_permille: 350,
			angle_centideg: -3000,
			spacing_permille: 250,
		},
		points: vec![point(1, 2)],
	};
	let expected = [
		3, 80, 20, 0, 0, //
		1, 94, 1, 72, 244, 250, 0, //
		4, 1, 0, 0, 0, 2, 0, 0, 0,
	];
	assert_eq!(stamped.encode(), expected);
	assert_eq!(MaskShape::decode(&mut expected.as_slice()).unwrap(), stamped);
	let follow = MaskFollow { pan: "p".into(), width: 100, brush: Some(slanted_nib()) };
	assert_eq!(follow.encode(), vec![4, b'p', 100, 0, 0, 0, 1, 0, 94, 1, 72, 244, 250, 0]);
	assert_eq!(MaskFollow::decode(&mut follow.encode().as_slice()).unwrap(), follow);
}

#[test]
fn a_followed_brush_reads_from_json_and_is_absent_when_left_out() {
	let round: MaskFollow = serde_json::from_str(r#"{"pan":"pan","width":5200}"#).unwrap();
	assert_eq!(round, MaskFollow { pan: "pan".into(), width: 5200, brush: None });
	let json = r#"{"pan":"pan","width":5200,"brush":{"tip":"Ellipse","aspectPermille":350,"angleCentideg":-3000,"spacingPermille":250}}"#;
	let stamped: MaskFollow = serde_json::from_str(json).unwrap();
	assert_eq!(stamped, MaskFollow { pan: "pan".into(), width: 5200, brush: Some(slanted_nib()) });
}

/// Stamps fall every spacing along the line, the distance carried across its
/// corners, with one more on a last point the spacing leaves bare.
#[test]
fn stamps_fall_every_spacing_along_the_line_and_on_its_ends() {
	let every = |spacing_permille| MaskBrush { spacing_permille, ..slanted_nib() };
	let straight = [point(0, 0), point(1000, 0)];
	assert_eq!(
		every(1000).stamps(400, &straight),
		vec![(0.0, 0.0), (400.0, 0.0), (800.0, 0.0), (1000.0, 0.0)]
	);
	assert_eq!(every(1000).stamps(500, &straight), vec![(0.0, 0.0), (500.0, 0.0), (1000.0, 0.0)]);
	let corner = [point(0, 0), point(300, 0), point(300, 300)];
	assert_eq!(every(1000).stamps(400, &corner), vec![(0.0, 0.0), (300.0, 100.0), (300.0, 300.0)]);
	assert_eq!(every(1000).stamps(400, &[point(7, 9)]), vec![(7.0, 9.0)]);
	assert!(every(1000).stamps(400, &[]).is_empty());
	// A spacing of zero is clamped to a twentieth of the width.
	assert_eq!(every(0).stamps(1000, &straight).len(), 21);
}

/// A one-stamp line over a 10 by 2 logical pixel node, one row of samples:
/// the tip lying flat covers three, turned upright only the one under it.
#[test]
fn a_stamp_covers_the_samples_its_turned_tip_covers() {
	let nib = |tip, angle_centideg| MaskShape::Stamped {
		width: 400,
		brush: MaskBrush { tip, aspect_permille: 250, angle_centideg, spacing_permille: 1000 },
		points: vec![point(500, 100)],
	};
	assert_eq!(cleared_share(&[nib(MaskBrushTip::Ellipse, 0)], 1000, 200), (3, 5));
	assert_eq!(cleared_share(&[nib(MaskBrushTip::Ellipse, 9000)], 1000, 200), (1, 5));
	// Over two rows, a flat tip 8 long and 2 thick centred between them: the
	// ellipse reaches the rows only at its middle, the capsule along its
	// straight sides.
	let flat = |tip| MaskShape::Stamped {
		width: 800,
		brush: MaskBrush { tip, aspect_permille: 250, angle_centideg: 0, spacing_permille: 1000 },
		points: vec![point(500, 200)],
	};
	assert_eq!(cleared_share(&[flat(MaskBrushTip::Ellipse)], 1000, 400), (2, 10));
	assert_eq!(cleared_share(&[flat(MaskBrushTip::Capsule)], 1000, 400), (6, 10));
}

/// A share with every progress option set.
fn eager_share() -> ClearedShare {
	ClearedShare {
		at_permille: 600,
		action: "done".into(),
		while_down: OptionBool(Some(true)),
		rescue: Some(ShareRescue { after_ms: 1500, from_permille: 450 }),
	}
}

/// The progress options follow the action: the mid-stroke flag as one byte,
/// then the rescue's time and lower share. Haptics are the mask's last field.
#[test]
fn mask_progress_options_follow_what_they_extend_on_the_wire() {
	assert_eq!(
		eager_share().encode(),
		vec![88, 2, 16, b'd', b'o', b'n', b'e', 1, 1, 0xDC, 0x05, 0, 0, 0xC2, 0x01]
	);
	let haptics =
		MaskHaptics { every_permille: 100, step: Haptic::Tick, reach: Some(Haptic::Success) };
	assert_eq!(haptics.encode(), vec![100, 0, 0, 1, 4]);
	let mask = Mask {
		haptics: Some(haptics),
		cleared_share: Some(eager_share()),
		..mask(every_shape(), None, None)
	};
	let bytes = Modifier::Mask(mask.clone()).encode();
	assert!(bytes.ends_with(&[1, 100, 0, 0, 1, 4]));
	assert_eq!(Modifier::decode(&mut bytes.as_slice()).unwrap(), Modifier::Mask(mask));
}

#[test]
fn mask_progress_options_read_from_json_and_are_off_when_left_out() {
	let bare: ClearedShare = serde_json::from_str(r#"{"atPermille":600,"action":"done"}"#).unwrap();
	assert_eq!(bare, ClearedShare { while_down: OptionBool(None), rescue: None, ..eager_share() });
	let json = r#"{"atPermille":600,"action":"done","whileDown":true,
		"rescue":{"afterMs":1500,"fromPermille":450}}"#;
	assert_eq!(serde_json::from_str::<ClearedShare>(json).unwrap(), eager_share());
	let haptics: MaskHaptics =
		serde_json::from_str(r#"{"everyPermille":100,"step":"Tick","reach":"Success"}"#).unwrap();
	assert_eq!(
		haptics,
		MaskHaptics { every_permille: 100, step: Haptic::Tick, reach: Some(Haptic::Success) }
	);
}

/// A step plays once for a frame however many it passed, counted from
/// multiples of the step, never for a cover that stood still or shrank.
#[test]
fn a_step_plays_when_the_cover_passes_a_multiple_of_the_step() {
	let every = |every_permille| MaskHaptics { every_permille, step: Haptic::Tick, reach: None };
	assert_eq!(every(100).steps_passed(95, 105), 1);
	assert_eq!(every(100).steps_passed(100, 199), 0);
	assert_eq!(every(100).steps_passed(99, 100), 1);
	assert_eq!(every(100).steps_passed(0, 1000), 10);
	assert_eq!(every(100).steps_passed(500, 400), 0);
	assert_eq!(every(0).steps_passed(0, 1000), 0);
}

#[test]
fn a_press_can_scale_each_axis_and_reads_from_json() {
	let squash =
		PanFeel { press_scale_permille: 1080, press_scale_y_permille: Some(920), ..full_feel() };
	let bytes = squash.encode();
	assert!(bytes.ends_with(&[1, 0x98, 0x03]));
	assert!(full_feel().encode().ends_with(&[0]));
	assert_eq!(PanFeel::decode(&mut bytes.as_slice()).unwrap(), squash);
	let json = r#"{"pressScalePermille":1080,"pressMs":133,"unpressMs":150,"springOmegaCenti":1800,
		"springDampingCenti":80,"approachMs":117,"approachReference":4200,"popPermille":[1059,1132,1192,1237,1000],
		"popOffset":333,"pressScaleYPermille":920}"#;
	assert_eq!(serde_json::from_str::<PanFeel>(json).unwrap(), squash);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn mask_progress_and_an_axis_press_round_trip_through_uniffi() {
	let mask = Modifier::Mask(Mask {
		haptics: Some(MaskHaptics { every_permille: 50, step: Haptic::Light, reach: None }),
		cleared_share: Some(eager_share()),
		..mask(every_shape(), None, None)
	});
	let ffi = <Modifier as uniffi::Lower<crate::UniFfiTag>>::lower(mask.clone());
	assert_eq!(<Modifier as uniffi::Lift<crate::UniFfiTag>>::try_lift(ffi).unwrap(), mask);
	let feel = PanFeel { press_scale_y_permille: Some(920), ..full_feel() };
	let ffi = <PanFeel as uniffi::Lower<crate::UniFfiTag>>::lower(feel.clone());
	assert_eq!(<PanFeel as uniffi::Lift<crate::UniFfiTag>>::try_lift(ffi).unwrap(), feel);
}

const LINEAR: Curve = Curve { x1: 0, y1: 0, x2: 1000, y2: 1000 };
const EASE: Curve = Curve { x1: 250, y1: 100, x2: 250, y2: 1000 };

/// Rises from the start, holds, and falls a little before the end.
fn rise_and_settle(repeat: u16, alternate: Option<bool>) -> Keyframes {
	Keyframes {
		frames: vec![
			Keyframe { at_ms: 0, value: 0, curve: LINEAR },
			Keyframe { at_ms: 100, value: 1000, curve: LINEAR },
			Keyframe { at_ms: 300, value: 600, curve: LINEAR },
		],
		repeat,
		alternate: OptionBool(alternate),
	}
}

/// A node that springs in from below, faded and small, and pulses for ever.
fn entering_and_pulsing() -> Vec<Modifier> {
	vec![
		Modifier::Enter(Enter {
			from: Pose {
				opacity_permille: 0,
				scale_permille: 900,
				x: 0,
				y: 1200,
				rotation_centideg: -500,
			},
			motion: Motion::Spring(Spring { omega_centi: 1800, damping_centi: 80 }),
			delay_ms: 50,
		}),
		Modifier::Animate(Track {
			property: AnimatedProperty::Scale,
			play: TrackPlay::Keyframes(rise_and_settle(0, Some(true))),
		}),
		Modifier::Animate(Track {
			property: AnimatedProperty::OffsetX,
			play: TrackPlay::Follow {
				target: -400,
				motion: Motion::Timed { ms: 250, curve: EASE },
			},
		}),
	]
}

/// `Enter` and `Animate` follow `Mask`: the pose's fields in order, then the
/// motion and the delay.
#[test]
fn enter_and_animate_are_the_modifiers_after_mask() {
	let [enter, scale, follow] = entering_and_pulsing().try_into().unwrap();
	assert_eq!(
		enter.encode(),
		vec![
			21, 0, 0, 0x84, 0x03, 0, 0, 0, 0, 0xB0, 0x04, 0, 0, 0x0C, 0xFE, 0xFF, 0xFF, //
			1, 0x08, 0x07, 80, 0, //
			50, 0,
		]
	);
	assert_eq!(scale.encode()[..3], [22, 1, 0]);
	// Follow is the second play: the target, then a timed motion and its curve.
	assert_eq!(
		follow.encode(),
		vec![
			22, 2, 1, 0x70, 0xFE, 0xFF, 0xFF, 0, 0xFA, 0, 0, 0, 0xFA, 0, 100, 0, 0xFA, 0, 0xE8,
			0x03
		]
	);
}

#[test]
fn entering_and_animated_nodes_round_trip_through_scale_json_and_uniffi() {
	let node = RendererNode::Box {
		modifiers: entering_and_pulsing(),
		props: BoxProps { content_alignment: None },
		children: vec![renderer_node()],
	};
	assert_eq!(RendererNode::decode(&mut node.encode().as_slice()).unwrap(), node);
	let json = r#"[
		{"tag":"Enter","value":{"from":{"opacityPermille":0,"scalePermille":900,"x":0,"y":1200,"rotationCentideg":-500},
			"motion":{"tag":"Spring","value":{"omegaCenti":1800,"dampingCenti":80}},"delayMs":50}},
		{"tag":"Animate","value":{"property":"Scale","play":{"tag":"Keyframes","value":{"frames":[
			{"atMs":0,"value":0,"curve":{"x1":0,"y1":0,"x2":1000,"y2":1000}},
			{"atMs":100,"value":1000,"curve":{"x1":0,"y1":0,"x2":1000,"y2":1000}},
			{"atMs":300,"value":600,"curve":{"x1":0,"y1":0,"x2":1000,"y2":1000}}],"repeat":0,"alternate":true}}}},
		{"tag":"Animate","value":{"property":"OffsetX","play":{"tag":"Follow","value":{"target":-400,
			"motion":{"tag":"Timed","value":{"ms":250,"curve":{"x1":250,"y1":100,"x2":250,"y2":1000}}}}}}}
	]"#;
	assert_eq!(serde_json::from_str::<Vec<Modifier>>(json).unwrap(), entering_and_pulsing());
	#[cfg(not(target_arch = "wasm32"))]
	{
		let ffi = <RendererNode as uniffi::Lower<crate::UniFfiTag>>::lower(node.clone());
		assert_eq!(<RendererNode as uniffi::Lift<crate::UniFfiTag>>::try_lift(ffi).unwrap(), node);
	}
}

#[test]
fn a_curve_eases_as_css_cubic_bezier_does() {
	let close = |a: f64, b: f64| (a - b).abs() < 1e-3;
	assert!(close(LINEAR.ease(0.25), 0.25));
	assert!(close(EASE.ease(0.0), 0.0));
	assert!(close(EASE.ease(1.0), 1.0));
	// CSS `ease` at half time is about 80% of the way.
	assert!(close(EASE.ease(0.5), 0.8024), "{}", EASE.ease(0.5));
	// Time outside 0 to 1 is clamped; progress may overshoot.
	assert!(close(EASE.ease(2.0), 1.0));
	let overshoot = Curve { x1: 340, y1: 1560, x2: 640, y2: 1000 };
	assert!(overshoot.ease(0.6) > 1.0);
}

/// Hosts draw whole units; the easing's own rounding stays below them.
fn rounded(value: Option<f64>) -> Option<i64> {
	value.map(|value| value.round() as i64)
}

#[test]
fn keyframes_play_runs_and_hold_where_the_last_run_ended() {
	let once = rise_and_settle(2, None);
	assert_eq!(rounded(once.value_at(50)), Some(500));
	assert_eq!(rounded(once.value_at(100)), Some(1000));
	assert_eq!(rounded(once.value_at(200)), Some(800));
	// The second run starts over from the first keyframe.
	assert_eq!(rounded(once.value_at(350)), Some(500));
	assert_eq!(rounded(once.value_at(600)), Some(600));
	assert_eq!(rounded(once.value_at(60_000)), Some(600));
	// Alternating, the second run plays backwards and ends at the start.
	let back = rise_and_settle(2, Some(true));
	assert_eq!(rounded(back.value_at(300)), Some(600));
	assert_eq!(rounded(back.value_at(350)), Some(700));
	assert_eq!(rounded(back.value_at(600)), Some(0));
	// For ever: the run repeats.
	assert_eq!(rounded(rise_and_settle(0, None).value_at(3050)), Some(500));
}

#[test]
fn keyframes_out_of_order_or_at_one_time_jump() {
	let frames = |times: [(u32, i32); 3]| Keyframes {
		frames: times.map(|(at_ms, value)| Keyframe { at_ms, value, curve: LINEAR }).to_vec(),
		repeat: 1,
		alternate: OptionBool(None),
	};
	// The last keyframe, earlier than the one before it, is taken at that
	// one's time and holds there.
	let late = frames([(0, 0), (200, 1000), (100, 500)]);
	assert_eq!(rounded(late.value_at(100)), Some(500));
	assert_eq!(rounded(late.value_at(200)), Some(500));
	assert_eq!(rounded(late.value_at(900)), Some(500));
	let still = frames([(0, 7), (0, 8), (0, 9)]);
	assert_eq!(rounded(still.value_at(0)), Some(9));
	assert_eq!(
		Keyframes { frames: Vec::new(), repeat: 0, alternate: OptionBool(None) }.value_at(5),
		None
	);
}

/// Debris from under a finger: two shapes and an image, falling and fading,
/// with a burst for every kind of trigger.
fn debris() -> Emitter {
	Emitter {
		looks: vec![
			ParticleLook::Circle { diameter: 300, color: 0xC0C4_CCFF },
			ParticleLook::Rectangle { width: 400, height: 200, color: 0x8A8F_99FF },
			ParticleLook::Image {
				source: ImageSource::Archive("particle.png".into()),
				width: 600,
				height: 600,
			},
		],
		source: EmitSource::Pan("pan".into()),
		rate_per_s: 0,
		rate_per_100pt: 12,
		life_ms: 700,
		speed: ValueRange { min: 4000, max: 12_000 },
		angle_centideg: -9000,
		spread_centideg: 12_000,
		gravity: 90_000,
		spin: ValueRange { min: -36_000, max: 36_000 },
		fade_from_permille: 600,
		end_scale_permille: 400,
		max_alive: 160,
		bursts: vec![
			Burst { count: 40, on: BurstTrigger::Action("done".into()) },
			Burst { count: 8, on: BurstTrigger::Appear },
			Burst { count: 20, on: BurstTrigger::Change(3) },
		],
	}
}

/// A still emitter of one dot from a point, bursting once on appearing.
fn one_dot() -> Emitter {
	Emitter {
		looks: vec![ParticleLook::Circle { diameter: 100, color: 0xFFFF_FFFF }],
		source: EmitSource::Point(point(500, -200)),
		rate_per_s: 2,
		rate_per_100pt: 0,
		life_ms: 1000,
		speed: ValueRange { min: 0, max: 0 },
		angle_centideg: 0,
		spread_centideg: 0,
		gravity: 0,
		spin: ValueRange { min: 0, max: 0 },
		fade_from_permille: 1000,
		end_scale_permille: 1000,
		max_alive: 1,
		bursts: vec![Burst { count: 1, on: BurstTrigger::Appear }],
	}
}

/// `Emit` is the modifier after `Animate`. A look, a source and a trigger
/// each lead with their variant's index.
#[test]
fn emit_is_the_modifier_after_animate() {
	let bytes = Modifier::Emit(one_dot()).encode();
	assert_eq!(
		bytes[..20],
		[23, 4, 0, 100, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 1, 0xF4, 1, 0, 0, 0x38, 0xFF, 0xFF, 0xFF]
	);
	// Each burst ends the encoding: one, of one particle, on appearing.
	assert!(bytes.ends_with(&[4, 1, 0, 0]));
	assert_eq!(BurstTrigger::Action("a".into()).encode(), vec![1, 4, b'a']);
	assert_eq!(BurstTrigger::Change(3).encode(), vec![2, 3, 0, 0, 0]);
	assert_eq!(EmitSource::Area.encode(), vec![2]);
}

#[test]
fn emitters_round_trip_through_scale_json_and_uniffi() {
	let node = RendererNode::Box {
		modifiers: vec![Modifier::Emit(debris()), Modifier::Emit(one_dot())],
		props: BoxProps { content_alignment: None },
		children: vec![renderer_node()],
	};
	assert_eq!(RendererNode::decode(&mut node.encode().as_slice()).unwrap(), node);
	let json = r#"{"looks":[{"tag":"Circle","value":{"diameter":100,"color":4294967295}}],
		"source":{"tag":"Point","value":{"x":500,"y":-200}},"ratePerS":2,"ratePer100pt":0,"lifeMs":1000,
		"speed":{"min":0,"max":0},"angleCentideg":0,"spreadCentideg":0,"gravity":0,"spin":{"min":0,"max":0},
		"fadeFromPermille":1000,"endScalePermille":1000,"maxAlive":1,"bursts":[{"count":1,"on":{"tag":"Appear"}}]}"#;
	assert_eq!(serde_json::from_str::<Emitter>(json).unwrap(), one_dot());
	let trigger: BurstTrigger = serde_json::from_str(r#"{"tag":"Action","value":"done"}"#).unwrap();
	assert_eq!(trigger, BurstTrigger::Action("done".into()));
	#[cfg(not(target_arch = "wasm32"))]
	{
		let ffi = <RendererNode as uniffi::Lower<crate::UniFfiTag>>::lower(node.clone());
		assert_eq!(<RendererNode as uniffi::Lift<crate::UniFfiTag>>::try_lift(ffi).unwrap(), node);
	}
}

/// A particle moves at its speed, falls with gravity, fades after its share
/// of life, shrinks towards its end scale, and is gone at the end.
#[test]
fn a_particle_moves_falls_fades_and_shrinks_over_its_life() {
	let falling = Emitter {
		life_ms: 2000,
		gravity: 200,
		fade_from_permille: 500,
		end_scale_permille: 0,
		..one_dot()
	};
	assert_eq!(falling.place_at((10.0, 20.0), 0.0, 0.0, 0), Some((10.0, 20.0, 1.0, 1.0)));
	assert_eq!(falling.place_at((0.0, 0.0), 0.0, 0.0, 1000), Some((0.0, 100.0, 1.0, 0.5)));
	assert_eq!(falling.place_at((0.0, 0.0), 1000.0, 0.0, 1500), Some((1500.0, 225.0, 0.5, 0.25)));
	assert_eq!(falling.place_at((0.0, 0.0), 0.0, 0.0, 2000), None);
	// Straight down at 1000 a second, with no gravity.
	let (_, y, ..) = one_dot()
		.place_at((0.0, 0.0), 1000.0, core::f64::consts::FRAC_PI_2, 500)
		.unwrap();
	assert!((y - 500.0).abs() < 1e-9);
	// A fade from 1000 never fades.
	assert_eq!(one_dot().place_at((0.0, 0.0), 0.0, 0.0, 999).map(|place| place.2), Some(1.0));
}

#[test]
fn a_range_picks_between_its_ends_either_way_round() {
	assert_eq!(ValueRange { min: 4000, max: 12_000 }.pick(0.25), 6000.0);
	assert_eq!(ValueRange { min: 12_000, max: 4000 }.pick(0.25), 6000.0);
	assert_eq!(ValueRange { min: -10, max: 10 }.pick(2.0), 10.0);
}

/// A particle image travels in the tree as an `Image` node's does, so the
/// same bytes are refused; a refused look draws no particle.
#[test]
fn refused_inline_particle_images_are_emptied() {
	let png = b"\x89PNG\r\n\x1a\nrest".to_vec();
	let emitting = |image: Vec<u8>| RendererNode::Gesture {
		modifiers: vec![Modifier::Emit(Emitter {
			looks: vec![
				ParticleLook::Image { source: ImageSource::Inline(image), width: 600, height: 600 },
				ParticleLook::Image {
					source: ImageSource::Inline(png.clone()),
					width: 600,
					height: 600,
				},
			],
			..one_dot()
		})],
		props: GestureProps {
			tap_action: None,
			pan: None,
			accessibility_label: None,
			enabled: OptionBool(None),
			haptic_on_complete: OptionBool(None),
			key: None,
			feel: None,
		},
		children: Vec::new(),
	};
	let mut node = emitting(b"<svg/>".to_vec());
	node.clear_refused_inline_images();
	assert_eq!(node, emitting(Vec::new()));
}

/// A back layer that drifts against the tilt and lags behind a dragged card.
fn back_layer() -> Parallax {
	Parallax {
		depth_permille: -300,
		tilt: Some(TiltParallax { travel: 2400, max_tilt_centideg: 2000, settle_ms: 3000 }),
		pan: Some("card".into()),
	}
}

/// `Parallax` is the modifier after `Emit`: the depth, then the tilt and the
/// pan, each optional.
#[test]
fn parallax_is_the_modifier_after_emit() {
	assert_eq!(
		Modifier::Parallax(back_layer()).encode(),
		vec![
			24, 0xD4, 0xFE, //
			1, 0x60, 0x09, 0, 0, 0xD0, 0x07, 0xB8, 0x0B, //
			1, 16, b'c', b'a', b'r', b'd',
		]
	);
	let still = Parallax { depth_permille: 1000, tilt: None, pan: None };
	assert_eq!(Modifier::Parallax(still).encode(), vec![24, 0xE8, 0x03, 0, 0]);
}

#[test]
fn parallax_round_trips_through_scale_json_and_uniffi() {
	let node = RendererNode::Box {
		modifiers: vec![Modifier::Parallax(back_layer())],
		props: BoxProps { content_alignment: None },
		children: vec![renderer_node()],
	};
	assert_eq!(RendererNode::decode(&mut node.encode().as_slice()).unwrap(), node);
	let json = r#"{"tag":"Parallax","value":{"depthPermille":-300,
		"tilt":{"travel":2400,"maxTiltCentideg":2000,"settleMs":3000},"pan":"card"}}"#;
	assert_eq!(serde_json::from_str::<Modifier>(json).unwrap(), Modifier::Parallax(back_layer()));
	let bare: Parallax = serde_json::from_str(r#"{"depthPermille":500}"#).unwrap();
	assert_eq!(bare, Parallax { depth_permille: 500, tilt: None, pan: None });
	#[cfg(not(target_arch = "wasm32"))]
	{
		let ffi = <RendererNode as uniffi::Lower<crate::UniFfiTag>>::lower(node.clone());
		assert_eq!(<RendererNode as uniffi::Lift<crate::UniFfiTag>>::try_lift(ffi).unwrap(), node);
	}
}

/// The tilt moves in proportion up to its clamp, the pan's offset adds to it,
/// and the depth scales and turns the sum.
#[test]
fn parallax_scales_the_tilt_and_the_pan_by_its_depth() {
	let front = Parallax { depth_permille: 1000, ..back_layer() };
	assert_eq!(front.offset((10.0, -5.0), (0.0, 0.0)), (1200.0, -600.0));
	assert_eq!(front.offset((45.0, -90.0), (0.0, 0.0)), (2400.0, -2400.0));
	assert_eq!(front.offset((10.0, 0.0), (300.0, 100.0)), (1500.0, 100.0));
	assert_eq!(back_layer().offset((10.0, 0.0), (300.0, 100.0)), (-450.0, -30.0));
	// With no tilt to follow, or none to measure, only the pan moves it.
	let pan_only = Parallax { tilt: None, ..front.clone() };
	assert_eq!(pan_only.offset((10.0, 10.0), (300.0, 100.0)), (300.0, 100.0));
	let flat = Parallax {
		tilt: Some(TiltParallax { travel: 2400, max_tilt_centideg: 0, settle_ms: 0 }),
		..pan_only.clone()
	};
	assert_eq!(flat.offset((10.0, 10.0), (0.0, 0.0)), (0.0, 0.0));
	// With no pan, the pan's offset is not followed.
	let tilt_only = Parallax { pan: None, ..front };
	assert_eq!(tilt_only.offset((0.0, 0.0), (300.0, 100.0)), (0.0, 0.0));
}
