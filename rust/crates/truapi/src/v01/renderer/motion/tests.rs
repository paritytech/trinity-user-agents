use parity_scale_codec::Compact;

use super::*;
use crate::v01::renderer::{BoxProps, DrivenProperty, KeyframeCurve, PanAxis};

fn pan(id: &str) -> Modifier {
	Modifier::Pan(Pan {
		id: id.to_string(),
		axis: PanAxis::Horizontal,
		distance: -1000,
		value: 0,
		max_steps_per_gesture: Some(1),
		period: Some(2),
		animation: None,
		action: Some("turned".to_string()),
		label: None,
	})
}

fn keyframe(at: f32, value: f32) -> Keyframe {
	Keyframe { at: at.to_bits(), value: value.to_bits(), curve: KeyframeCurve::Linear }
}

fn drive(source: &str, keyframes: Vec<Keyframe>) -> Modifier {
	Modifier::Drive(Drive {
		source: source.to_string(),
		property: DrivenProperty::RotationY,
		keyframes,
	})
}

fn turn() -> Modifier {
	drive("turn", vec![keyframe(0.0, 0.0), keyframe(2.0, 360.0)])
}

fn boxed(modifiers: Vec<Modifier>, children: Vec<RendererNode>) -> RendererNode {
	RendererNode::Box { modifiers, props: BoxProps { content_alignment: None }, children }
}

fn cleared(mut node: RendererNode) -> RendererNode {
	node.clear_invalid_motion();
	node
}

/// A turning card: one pan on the card, drives on both faces and on the card
/// itself. Everything in it is runnable, so nothing may be taken away.
#[test]
fn a_valid_turning_card_is_left_alone() {
	let card = boxed(
		vec![Modifier::Perspective(Compact(1200)), pan("turn"), turn()],
		vec![boxed(vec![turn()], Vec::new()), boxed(vec![turn()], Vec::new())],
	);

	assert_eq!(cleared(card.clone()), card);
}

/// A drive may sit before its pan in the tree, such as on a sibling drawn
/// first, because the host evaluates it from the value, not from its place.
#[test]
fn a_drive_may_precede_its_pan() {
	let card = boxed(
		Vec::new(),
		vec![boxed(vec![turn()], Vec::new()), boxed(vec![pan("turn")], Vec::new())],
	);

	assert_eq!(cleared(card.clone()), card);
}

/// Two pans with one id would leave a drive unable to say which it follows,
/// so the first in depth-first order keeps the id and the drive follows it.
#[test]
fn a_repeated_pan_id_keeps_the_first() {
	let card =
		|inner: Vec<Modifier>| boxed(vec![pan("turn"), turn()], vec![boxed(inner, Vec::new())]);

	assert_eq!(cleared(card(vec![pan("turn")])), card(Vec::new()));
}

#[test]
fn a_drive_naming_no_pan_is_dropped() {
	let card = |modifiers: Vec<Modifier>| boxed(modifiers, Vec::new());

	assert_eq!(
		cleared(card(vec![pan("turn"), drive("other", vec![keyframe(0.0, 0.0)])])),
		card(vec![pan("turn")])
	);
}

/// A pan that cannot move or cannot wrap is dropped, and the drives that
/// followed it go with it rather than reading a value nothing moves.
#[test]
fn an_unrunnable_pan_is_dropped_with_its_drives() {
	let Modifier::Pan(base) = pan("turn") else { unreachable!() };
	for broken in [Pan { distance: 0, ..base.clone() }, Pan { period: Some(0), ..base.clone() }] {
		let card = boxed(vec![Modifier::Pan(broken), turn(), Modifier::Opacity(255)], Vec::new());

		assert_eq!(cleared(card), boxed(vec![Modifier::Opacity(255)], Vec::new()));
	}
}

/// Hosts interpolate between neighbouring keyframes, so each host would read
/// keyframes out of order, or ones that are not numbers, differently.
#[test]
fn a_drive_with_keyframes_a_host_cannot_read_is_dropped() {
	let broken = [
		Vec::new(),
		vec![keyframe(1.0, 0.0), keyframe(0.0, 0.0)],
		vec![keyframe(0.5, 0.0), keyframe(0.5, 1.0)],
		vec![keyframe(f32::NAN, 0.0)],
		vec![keyframe(0.0, f32::INFINITY)],
		(0..=u8::try_from(MAX_KEYFRAMES).unwrap())
			.map(|at| keyframe(f32::from(at), 0.0))
			.collect(),
	];
	for keyframes in broken {
		let card = boxed(vec![pan("turn"), drive("turn", keyframes.clone())], Vec::new());

		assert_eq!(cleared(card), boxed(vec![pan("turn")], Vec::new()), "{keyframes:?}");
	}
}

/// Limits bound the work a host does per frame. What fits is kept in
/// depth-first order, so the same tree loses the same motion on every host.
#[test]
fn pans_and_drives_past_the_limits_are_dropped() {
	let pans: Vec<_> = (0..=MAX_PANS).map(|index| pan(&format!("pan{index}"))).collect();
	let mut kept_pans = pans.clone();
	kept_pans.pop();
	assert_eq!(cleared(boxed(pans, Vec::new())), boxed(kept_pans, Vec::new()));

	let drives: Vec<_> = (0..=MAX_DRIVES).map(|_| turn()).collect();
	let children: Vec<_> =
		drives.iter().map(|drive| boxed(vec![drive.clone()], Vec::new())).collect();
	let mut kept_children = children.clone();
	kept_children[MAX_DRIVES] = boxed(Vec::new(), Vec::new());
	assert_eq!(
		cleared(boxed(vec![pan("turn")], children)),
		boxed(vec![pan("turn")], kept_children)
	);
}

/// Motion inside a `Materials` node's content is checked like any other.
#[test]
fn motion_inside_materials_content_is_checked() {
	let tree = |modifiers: Vec<Modifier>| RendererNode::Materials {
		props: crate::v01::renderer::MaterialsProps { layers: Vec::new() },
		children: vec![boxed(modifiers, Vec::new())],
	};

	assert_eq!(cleared(tree(vec![drive("missing", vec![keyframe(0.0, 0.0)])])), tree(Vec::new()));
}
