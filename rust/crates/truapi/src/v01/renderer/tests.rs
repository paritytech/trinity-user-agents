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
		MaskShape::All,
	]
}

#[test]
fn a_masked_node_round_trips_with_every_shape_follow_fade_and_share() {
	for mask in [
		mask(Vec::new(), None, None),
		mask(every_shape(), Some(MaskFollow { pan: "pan".into(), width: 5200 }), Some(350)),
		Mask {
			key: None,
			mode: MaskMode::Show,
			shapes: every_shape(),
			follow: None,
			fade_ms: Some(0),
			cleared_share: Some(ClearedShare { at_permille: 550, action: "cleared".into() }),
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
	});
	assert_eq!(empty.encode(), vec![20, 0, 0, 0, 0, 0, 0]);
	let show = Modifier::Mask(Mask {
		key: Some("k".into()),
		mode: MaskMode::Show,
		shapes: vec![MaskShape::All],
		follow: Some(MaskFollow { pan: "p".into(), width: 100 }),
		fade_ms: Some(350),
		cleared_share: Some(ClearedShare { at_permille: 550, action: "a".into() }),
	});
	// The share follows the fade: present, 550 as a little-endian u16, the action.
	assert_eq!(
		show.encode(),
		vec![20, 1, 4, b'k', 1, 4, 0, 1, 4, b'p', 100, 0, 0, 0, 1, 94, 1, 1, 38, 2, 4, b'a']
	);
	assert_eq!(Modifier::decode(&mut show.encode().as_slice()).unwrap(), show);
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
			Some(MaskFollow { pan: "pan".into(), width: 5200 }),
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
	let share = |at_permille| ClearedShare { at_permille, action: "a".into() };
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
