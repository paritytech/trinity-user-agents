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
	assert_eq!(node.encode(), vec![13, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0]);
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
				key: Some("thumb".into()),
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
	let node = RendererNode::SetOffset { key: "thumb".into(), x: 120, y: -2 };
	let expected = [14, 20, b't', b'h', b'u', b'm', b'b', 120, 0, 0, 0, 254, 255, 255, 255];
	assert_eq!(node.encode(), expected);
	assert_eq!(RendererNode::decode(&mut expected.as_slice()).unwrap(), node);
}

/// An offset update is eleven bytes plus its key, against hundreds for the
/// footer tree it moves a part of.
#[test]
fn set_offset_is_smaller_than_any_tree_holding_its_gesture() {
	let offset = RendererNode::SetOffset { key: "thumb".into(), x: 120, y: 0 };
	let tree = RendererNode::Gesture {
		modifiers: Vec::new(),
		props: GestureProps {
			tap_action: None,
			pan: None,
			accessibility_label: None,
			enabled: OptionBool(None),
			haptic_on_complete: OptionBool(None),
			key: Some("thumb".into()),
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
		RendererNode::SetOffset { key: "thumb".into(), x: -7, y: 3 },
		RendererNode::Gesture {
			modifiers: Vec::new(),
			props: GestureProps {
				tap_action: None,
				pan: None,
				accessibility_label: None,
				enabled: OptionBool(None),
				haptic_on_complete: OptionBool(None),
				key: Some("thumb".into()),
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

/// Every modifier that styles by value, as the existing slider's pill uses them.
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
		Modifier::FadeWithPan(PanFade { key: "thumb".into(), rate_permille: 1400 }),
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

fn scratch_tree(cleared: Vec<u8>) -> RendererNode {
	RendererNode::Gesture {
		modifiers: vec![Modifier::Width(Compact(280)), Modifier::Height(Compact(300))],
		props: GestureProps {
			tap_action: None,
			pan: Some(PanGesture {
				axis: PanAxis::Free,
				drag: None,
				start_action: Some("scratch.start".into()),
				move_action: Some("scratch.move".into()),
				move_interval_ms: None,
				end_action: Some("scratch.end".into()),
				location: OptionBool(Some(true)),
			}),
			accessibility_label: Some("Scratch the ticket".into()),
			enabled: OptionBool(None),
			haptic_on_complete: OptionBool(None),
			key: Some("scratch".into()),
			feel: None,
		},
		children: vec![RendererNode::CoverGrid {
			modifiers: vec![Modifier::FillWidth(true)],
			props: CoverGridProps {
				key: "foil".into(),
				columns: 35,
				rows: 37,
				cleared,
				brush: None,
			},
			children: vec![renderer_node()],
		}],
	}
}

#[test]
fn a_scratch_card_round_trips_with_its_cover_and_location_pan() {
	for cleared in [Vec::new(), vec![0b1000_0001, 0, 0xff]] {
		let node = scratch_tree(cleared);
		assert_eq!(RendererNode::decode(&mut node.encode().as_slice()).unwrap(), node);
	}
}

#[test]
fn clear_cells_has_the_same_bytes_on_hosts_and_clients() {
	let node = RendererNode::ClearCells { key: "foil".into(), cells: vec![3, 300] };
	// Index 16, the key, then a compact count of two little-endian u16 cells.
	let expected = [16, 16, b'f', b'o', b'i', b'l', 8, 3, 0, 44, 1];
	assert_eq!(node.encode(), expected);
	assert_eq!(RendererNode::decode(&mut expected.as_slice()).unwrap(), node);
}

#[test]
fn a_cover_grid_keeps_its_wire_index() {
	let node = RendererNode::CoverGrid {
		modifiers: Vec::new(),
		props: CoverGridProps {
			key: "f".into(),
			columns: 2,
			rows: 1,
			cleared: vec![2],
			brush: None,
		},
		children: vec![RendererNode::Nil],
	};
	assert_eq!(node.encode(), vec![15, 0, 4, b'f', 2, 0, 1, 0, 4, 2, 0, 4, 0]);
}

/// A brush is the last of a cover grid's props: present, then the pan's key
/// and the width as a compact.
#[test]
fn a_cover_brush_follows_the_cleared_bits() {
	let node = RendererNode::CoverGrid {
		modifiers: Vec::new(),
		props: CoverGridProps {
			key: "f".into(),
			columns: 2,
			rows: 1,
			cleared: vec![2],
			brush: Some(CoverBrush { pan: "p".into(), width: Compact(52) }),
		},
		children: vec![RendererNode::Nil],
	};
	assert_eq!(node.encode(), vec![15, 0, 4, b'f', 2, 0, 1, 0, 4, 2, 1, 4, b'p', 208, 4, 0]);
	assert_eq!(RendererNode::decode(&mut node.encode().as_slice()).unwrap(), node);
}

/// The brush travels with the tree, never with a move: an answer is the same
/// few bytes whether the cover draws with a brush or with round holes.
#[test]
fn a_brushed_cover_answers_moves_with_the_same_clear_cells() {
	let mut tree = scratch_tree(vec![0; 162]);
	let RendererNode::Gesture { children, .. } = &mut tree else { unreachable!() };
	let RendererNode::CoverGrid { props, .. } = &mut children[0] else { unreachable!() };
	props.brush = Some(CoverBrush { pan: "scratch".into(), width: Compact(52) });
	assert_eq!(RendererNode::decode(&mut tree.encode().as_slice()).unwrap(), tree);
	let update = RendererNode::ClearCells { key: "foil".into(), cells: (0..8).collect() };
	assert_eq!(update.encode().len(), 23);
}

#[test]
fn cleared_bits_read_row_by_row_lowest_bit_first() {
	let props = CoverGridProps {
		key: "f".into(),
		columns: 4,
		rows: 3,
		cleared: vec![0b0000_0101, 0b0000_1000],
		brush: None,
	};
	let cleared: Vec<u32> = (0..12).filter(|&cell| props.is_cleared(cell)).collect();
	assert_eq!(cleared, vec![0, 2, 11]);
	assert!(!props.is_cleared(400), "a cell past the bytes is not cleared");
}

/// A move's answer stays a few bytes however much of the card is already
/// scratched, while the tree carrying the same cover grows with it.
#[test]
fn clearing_cells_is_far_smaller_than_the_tree_that_carries_them() {
	let update = RendererNode::ClearCells { key: "foil".into(), cells: (0..8).collect() };
	let tree = scratch_tree(vec![0xff; 162]);
	assert_eq!(update.encode().len(), 23);
	assert!(update.encode().len() * 10 < tree.encode().len());
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
		{"tag":"FadeWithPan","value":{"key":"thumb","ratePermille":1400}},
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

/// The existing slider's own feel.
fn slide_feel() -> PanFeel {
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
			key: Some("thumb".into()),
			feel: Some(slide_feel()),
		},
		children: vec![RendererNode::Nil],
	};
	assert_eq!(RendererNode::decode(&mut node.encode().as_slice()).unwrap(), node);
	let json = r#"{"pressScalePermille":1059,"pressMs":133,"unpressMs":150,"springOmegaCenti":1800,
		"springDampingCenti":80,"approachMs":117,"approachReference":4200,"popPermille":[1059,1132,1192,1237,1000],"popOffset":333}"#;
	assert_eq!(serde_json::from_str::<PanFeel>(json).unwrap(), slide_feel());
	#[cfg(not(target_arch = "wasm32"))]
	{
		let ffi = <RendererNode as uniffi::Lower<crate::UniFfiTag>>::lower(node.clone());
		assert_eq!(<RendererNode as uniffi::Lift<crate::UniFfiTag>>::try_lift(ffi).unwrap(), node);
	}
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn cover_grid_and_clear_cells_round_trip_through_uniffi() {
	let mut brushed = scratch_tree(vec![1, 2, 3]);
	if let RendererNode::Gesture { children, .. } = &mut brushed &&
		let RendererNode::CoverGrid { props, .. } = &mut children[0]
	{
		props.brush = Some(CoverBrush { pan: "scratch".into(), width: Compact(52) });
	}
	for node in [
		scratch_tree(vec![1, 2, 3]),
		brushed,
		RendererNode::ClearCells { key: "foil".into(), cells: vec![0, 1294] },
	] {
		let ffi = <RendererNode as uniffi::Lower<crate::UniFfiTag>>::lower(node.clone());
		let lifted = <RendererNode as uniffi::Lift<crate::UniFfiTag>>::try_lift(ffi).unwrap();
		assert_eq!(lifted, node);
	}
}
