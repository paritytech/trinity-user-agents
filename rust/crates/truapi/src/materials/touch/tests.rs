use parity_scale_codec::Compact;
use truapi::latest::{DragAxis, ReplayProps, TouchTrigger, TrailProps};

use super::*;

fn props(trail: Option<u64>, triggers: Vec<(TouchCondition, &str)>) -> TouchProps {
	TouchProps {
		drag: DragAxis::Any,
		trail: trail.map(|radius| TrailProps { radius: Compact(radius) }),
		triggers: triggers
			.into_iter()
			.map(|(when, action)| TouchTrigger { when, action: action.to_string() })
			.collect(),
		replay: None,
	}
}

fn marked_per_mille(area: &TouchArea) -> usize {
	let trail = area.trail().unwrap();
	let marked = trail.bytes.iter().filter(|cell| **cell == u8::MAX).count();
	marked * 1000 / trail.bytes.len()
}

/// The card is revealed at the sample that takes the scratched part to 60
/// percent, and a card already revealed never asks the product again.
#[test]
fn scratching_fires_revealed_once_at_the_sample_that_covers_600_per_mille() {
	let mut area =
		TouchArea::new(props(Some(20), vec![(TouchCondition::TrailCovers(600), "revealed")]), 300.0, 200.0, 0.0);
	let mut fired_at = Vec::new();
	let mut time = 0.0;
	for (stroke, y) in [20.0, 60.0, 100.0, 140.0, 180.0].into_iter().enumerate() {
		let before = marked_per_mille(&area);
		assert_eq!(area.touch(TouchPhase::Down, 0.0, y, time), Vec::<String>::new());
		assert!(marked_per_mille(&area) < 600 || before >= 600);
		for step in 1..=30 {
			time += 0.01;
			let before = marked_per_mille(&area);
			let actions = area.touch(TouchPhase::Move, f64::from(step) * 10.0, y, time);
			let crossed = before < 600 && marked_per_mille(&area) >= 600;
			let expected = if crossed { vec!["revealed".to_string()] } else { Vec::new() };
			assert_eq!(actions, expected, "stroke {stroke}, step {step}");
			if crossed {
				fired_at.push((stroke, step));
			}
		}
		assert_eq!(area.touch(TouchPhase::Up, 300.0, y, time), Vec::<String>::new());
	}

	assert_eq!(fired_at.len(), 1);
	assert_eq!(marked_per_mille(&area), 1000);
}

fn slider() -> TouchArea {
	let from = Region { x: 0, y: 0, width: 200, height: 1000 };
	let to = Region { x: 900, y: 0, width: 100, height: 1000 };
	TouchArea::new(props(None, vec![(TouchCondition::Drag { from, to }, "confirm")]), 300.0, 60.0, 0.0)
}

/// Every action each sample of a touch fires, from `down` across to `up` in
/// steps of 10 logical pixels.
fn drag(area: &mut TouchArea, down: f64, up: f64) -> Vec<Vec<String>> {
	let mut actions = vec![area.touch(TouchPhase::Down, down, 30.0, 0.0)];
	let mut x = down;
	while x + 10.0 < up {
		x += 10.0;
		actions.push(area.touch(TouchPhase::Move, x, 30.0, 0.0));
	}
	actions.push(area.touch(TouchPhase::Up, up, 30.0, 0.0));
	actions
}

#[test]
fn a_drag_from_the_start_to_the_end_confirms_once() {
	let mut area = slider();

	let actions = drag(&mut area, 15.0, 295.0);

	let first_at_the_end = actions.iter().position(|fired| !fired.is_empty()).unwrap();
	assert_eq!(actions[first_at_the_end], ["confirm"]);
	assert_eq!(first_at_the_end, 26, "the sample at 275 of 300 is the first past 900 per mille");
	assert!(actions[first_at_the_end + 1..].iter().all(Vec::is_empty));
	assert!(drag(&mut area, 15.0, 295.0).iter().all(Vec::is_empty));
}

#[test]
fn a_touch_that_goes_down_near_the_end_confirms_nothing() {
	let mut area = slider();

	assert!(drag(&mut area, 285.0, 299.0).iter().all(Vec::is_empty));
}

#[test]
fn a_drag_lifted_halfway_confirms_nothing() {
	let mut area = slider();

	assert!(drag(&mut area, 15.0, 150.0).iter().all(Vec::is_empty));
	assert_eq!(drag(&mut area, 15.0, 295.0).concat(), ["confirm"]);
}

#[test]
fn a_tap_fires_every_time_and_a_touch_that_moved_fires_nothing() {
	let region = Region { x: 0, y: 0, width: 1000, height: 1000 };
	let mut area = TouchArea::new(props(None, vec![(TouchCondition::Tap(region), "flip")]), 300.0, 200.0, 0.0);
	let tap = |area: &mut TouchArea, moved: f64| {
		let mut actions = area.touch(TouchPhase::Down, 100.0, 100.0, 0.0);
		actions.extend(area.touch(TouchPhase::Move, 100.0 + moved, 100.0, 0.1));
		actions.extend(area.touch(TouchPhase::Up, 100.0 + moved, 100.0, 0.2));
		actions
	};

	assert_eq!(tap(&mut area, 9.0), ["flip"]);
	assert_eq!(tap(&mut area, 0.0), ["flip"]);
	assert!(tap(&mut area, 10.0).is_empty());
}

#[test]
fn the_shader_values_follow_the_finger() {
	let mut area = TouchArea::new(props(None, Vec::new()), 300.0, 200.0, 0.0);
	let inputs = |touch: [f32; 4], state: [f32; 2]| TouchInputs {
		touch: touch.to_vec(),
		state: state.to_vec(),
		replay: 0.0,
	};

	assert_eq!(area.inputs(5.0), inputs([0.0; 4], [0.0, -1.0]));

	area.touch(TouchPhase::Down, 30.0, 50.0, 1.0);
	assert_eq!(area.inputs(1.25), inputs([0.1, 0.25, 0.1, 0.25], [1.0, 0.25]));

	area.touch(TouchPhase::Move, 150.0, 100.0, 1.5);
	assert_eq!(area.inputs(2.0), inputs([0.5, 0.5, 0.1, 0.25], [1.0, 1.0]));

	area.touch(TouchPhase::Up, 150.0, 100.0, 3.0);
	assert_eq!(area.inputs(3.5), inputs([0.5, 0.5, 0.1, 0.25], [0.0, 0.5]));

	area.touch(TouchPhase::Move, 600.0, -40.0, 4.0);
	assert_eq!(area.inputs(3.5), inputs([0.5, 0.5, 0.1, 0.25], [0.0, 0.5]));

	area.touch(TouchPhase::Down, 600.0, -40.0, 4.0);
	assert_eq!(area.inputs(4.0), inputs([1.0, 0.0, 1.0, 0.0], [1.0, 0.0]));
}

/// The design's replay clock: the story plays from the first draw, a finger
/// that goes down while it plays is ignored, and one that goes down after it
/// played out starts it again at the moment it went down, whenever it lifts.
#[test]
fn a_finger_down_replays_the_story_only_once_it_played_out() {
	let story = TouchProps { replay: Some(ReplayProps { length: 2200 }), ..props(None, Vec::new()) };
	let mut area = TouchArea::new(story, 300.0, 200.0, 10.0);
	let replay = |area: &TouchArea, now: f64| area.inputs(now).replay;

	assert_eq!(replay(&area, 11.0), 1.0);

	area.touch(TouchPhase::Down, 100.0, 100.0, 11.5);
	area.touch(TouchPhase::Up, 100.0, 100.0, 11.6);
	assert_eq!(replay(&area, 12.0), 2.0, "a tap while the story plays is ignored");

	area.touch(TouchPhase::Down, 100.0, 100.0, 13.0);
	area.touch(TouchPhase::Up, 100.0, 100.0, 13.75);
	assert_eq!(replay(&area, 13.75), 0.75, "the replay starts at the finger down");

	area.touch(TouchPhase::Down, 100.0, 100.0, 14.0);
	assert_eq!(replay(&area, 14.0), 1.0, "a second tap during the replay is ignored");
}

#[test]
fn a_node_without_a_replay_clock_reads_zero() {
	let mut area = TouchArea::new(props(None, Vec::new()), 300.0, 200.0, 10.0);
	area.touch(TouchPhase::Down, 100.0, 100.0, 20.0);

	assert_eq!(area.inputs(30.0).replay, 0.0);
}

#[test]
fn the_trail_is_128_cells_on_its_longer_side_and_marks_within_the_radius() {
	let mut area = TouchArea::new(props(Some(4), Vec::new()), 100.0, 300.0, 0.0);
	let empty = area.trail().unwrap();
	assert_eq!((empty.width, empty.height, empty.revision), (43, 128, 0));

	area.touch(TouchPhase::Down, 50.0, 150.0, 0.0);

	let trail = area.trail().unwrap();
	let marked: Vec<(u32, u32)> = (0..trail.height)
		.flat_map(|row| (0..trail.width).map(move |column| (column, row)))
		.filter(|(column, row)| trail.bytes[(row * trail.width + column) as usize] == u8::MAX)
		.collect();
	let cell = 300.0 / 128.0;
	assert!(!marked.is_empty());
	assert!(marked.iter().all(|(column, row)| {
		let x = (f64::from(*column) + 0.5) * (100.0 / 43.0);
		let y = (f64::from(*row) + 0.5) * cell;
		(x - 50.0).hypot(y - 150.0) <= 4.0
	}));
	assert_eq!(trail.revision, 1);
}
