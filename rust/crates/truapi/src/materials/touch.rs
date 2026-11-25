//! What one `Touch` node does with the finger: the values its materials read,
//! the trail it leaves and the triggers it fires, the same on every host.
//!
//! [`TouchArea`] holds no clock and does no I/O. The host feeds it finger
//! samples with their time and reads back the actions to send and the inputs
//! to draw with.

use truapi::latest::{Region, TouchCondition, TouchProps};

/// `Touch` nodes in one tree that take touch, at most.
pub const MAX_TREE_TOUCH_NODES: usize = 4;

/// Triggers in one `Touch` node, at most.
const MAX_TRIGGERS: usize = 8;

/// Cells on the longer side of a trail.
const TRAIL_LONG_SIDE_CELLS: usize = 128;

/// How far a finger moves, in logical pixels, before its touch is no tap.
const TAP_SLOP: f64 = 10.0;

const PER_MILLE: u16 = 1000;

/// Whether a `Touch` node with `props` meets the node limits: at most 8
/// triggers, regions inside the node, and a trail wherever one is counted.
pub fn props_valid(props: &TouchProps) -> bool {
	let within = |region: &Region| {
		u32::from(region.x) + u32::from(region.width) <= u32::from(PER_MILLE) &&
			u32::from(region.y) + u32::from(region.height) <= u32::from(PER_MILLE)
	};
	props.triggers.len() <= MAX_TRIGGERS &&
		props.triggers.iter().all(|trigger| match &trigger.when {
			TouchCondition::Tap(region) => within(region),
			TouchCondition::Drag { from, to } => within(from) && within(to),
			TouchCondition::TrailCovers(_) => props.trail.is_some(),
		})
}

/// What the finger does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TouchPhase {
	/// It went down inside the node.
	Down,
	/// It moved while down.
	Move,
	/// It lifted.
	Up,
	/// The touch went to something else, such as the scroll around the node.
	Cancel,
}

/// The values a node's touch arguments take in one frame.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TouchInputs {
	/// What fills a `Touch` argument: the last position, then where that touch
	/// went down, each in the node's unit square from its top left.
	pub touch: Vec<f32>,
	/// What fills a `TouchState` argument: 1 while down and 0 otherwise, then
	/// seconds since the finger last went down or lifted.
	pub state: Vec<f32>,
	/// What fills a `Replay` argument: seconds since the replay clock last
	/// started, or 0 for a node that keeps no replay clock.
	pub replay: f32,
}

/// The trail as an image: one byte a cell, 255 where marked, row after row
/// from the top left.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TrailImage {
	/// Cells a row.
	pub width: u32,
	/// Rows.
	pub height: u32,
	/// The cells.
	pub bytes: Vec<u8>,
	/// Grows each time a cell is marked, so a host uploads the image only
	/// when it moved.
	pub revision: u64,
}

/// One `Touch` node of a drawn tree, as long as its props and size stay.
pub struct TouchArea {
	props: TouchProps,
	width: f64,
	height: f64,
	trail: Option<Trail>,
	fired: Vec<bool>,
	contact: Option<Contact>,
	last: Option<Sample>,
	replay: Option<ReplayClock>,
}

/// When the story a `Replay` argument plays last started, and how long it plays.
struct ReplayClock {
	length: f64,
	started_at: f64,
}

/// The touch under way.
struct Contact {
	down: Point,
	at: Point,
	moved: f64,
}

/// The last thing the finger did, for the shader.
struct Sample {
	position: Point,
	down: Point,
	pressed: bool,
	changed_at: f64,
}

#[derive(Clone, Copy)]
struct Point {
	x: f64,
	y: f64,
}

impl TouchArea {
	/// A node with `props`, `width` by `height` logical pixels, before any
	/// touch, first drawn `at_seconds` on the host's clock.
	pub fn new(props: TouchProps, width: f64, height: f64, at_seconds: f64) -> Self {
		let width = width.max(1.0);
		let height = height.max(1.0);
		let trail = props.trail.as_ref().map(|trail| Trail::new(width, height, trail.radius.0));
		let fired = vec![false; props.triggers.len()];
		let replay = props.replay.as_ref().map(|replay| ReplayClock {
			length: f64::from(replay.length) / 1000.0,
			started_at: at_seconds,
		});
		Self { props, width, height, trail, fired, contact: None, last: None, replay }
	}

	/// Takes one finger sample at `x`, `y` logical pixels from the node's top
	/// left, `at_seconds` on the host's clock, and returns the actions it
	/// fires, in trigger order.
	pub fn touch(&mut self, phase: TouchPhase, x: f64, y: f64, at_seconds: f64) -> Vec<String> {
		let point = Point { x: x.clamp(0.0, self.width), y: y.clamp(0.0, self.height) };
		let mut fired = Vec::new();
		match phase {
			TouchPhase::Down => {
				if let Some(replay) = &mut self.replay &&
					at_seconds - replay.started_at > replay.length
				{
					replay.started_at = at_seconds;
				}
				self.contact = Some(Contact { down: point, at: point, moved: 0.0 });
				self.mark(point, point);
				self.last =
					Some(Sample { position: point, down: point, pressed: true, changed_at: at_seconds });
			},
			TouchPhase::Move | TouchPhase::Up => {
				let Some(mut contact) = self.contact.take() else {
					return fired;
				};
				self.mark(contact.at, point);
				let distance = (point.x - contact.down.x).hypot(point.y - contact.down.y);
				contact.moved = contact.moved.max(distance);
				contact.at = point;
				let lifted = phase == TouchPhase::Up;
				self.fire_on_contact(&contact, lifted, &mut fired);
				if let Some(last) = &mut self.last {
					last.position = point;
					if lifted {
						last.pressed = false;
						last.changed_at = at_seconds;
					}
				}
				if !lifted {
					self.contact = Some(contact);
				}
			},
			TouchPhase::Cancel => {
				if self.contact.take().is_some() &&
					let Some(last) = &mut self.last
				{
					last.pressed = false;
					last.changed_at = at_seconds;
				}
			},
		}
		self.fire_on_trail(&mut fired);
		fired
	}

	/// The touch arguments at `now_seconds` on the host's clock.
	pub fn inputs(&self, now_seconds: f64) -> TouchInputs {
		let since = |at: f64| (now_seconds - at).max(0.0) as f32;
		let replay = self.replay.as_ref().map_or(0.0, |replay| since(replay.started_at));
		let Some(last) = &self.last else {
			return TouchInputs { touch: vec![0.0; 4], state: vec![0.0, -1.0], replay };
		};
		let (position, down) = (self.unit(last.position), self.unit(last.down));
		TouchInputs {
			touch: vec![position[0], position[1], down[0], down[1]],
			state: vec![if last.pressed { 1.0 } else { 0.0 }, since(last.changed_at)],
			replay,
		}
	}

	/// The trail, when the node keeps one.
	pub fn trail(&self) -> Option<TrailImage> {
		self.trail.as_ref().map(Trail::image)
	}

	fn unit(&self, point: Point) -> [f32; 2] {
		[(point.x / self.width) as f32, (point.y / self.height) as f32]
	}

	fn mark(&mut self, from: Point, to: Point) {
		if let Some(trail) = &mut self.trail {
			trail.mark(from, to);
		}
	}

	fn inside(&self, point: Point, region: &Region) -> bool {
		let x = point.x / self.width * f64::from(PER_MILLE);
		let y = point.y / self.height * f64::from(PER_MILLE);
		let left = f64::from(region.x);
		let top = f64::from(region.y);
		(left..=left + f64::from(region.width)).contains(&x) &&
			(top..=top + f64::from(region.height)).contains(&y)
	}

	fn fire_on_contact(&mut self, contact: &Contact, lifted: bool, fired: &mut Vec<String>) {
		for (index, trigger) in self.props.triggers.iter().enumerate() {
			let fires = match &trigger.when {
				TouchCondition::Tap(region) => {
					lifted &&
						contact.moved < TAP_SLOP &&
						self.inside(contact.down, region) &&
						self.inside(contact.at, region)
				},
				TouchCondition::Drag { from, to } => {
					!self.fired[index] &&
						self.inside(contact.down, from) &&
						self.inside(contact.at, to)
				},
				TouchCondition::TrailCovers(_) => false,
			};
			if fires {
				self.fired[index] = true;
				fired.push(trigger.action.clone());
			}
		}
	}

	fn fire_on_trail(&mut self, fired: &mut Vec<String>) {
		let Some(trail) = &self.trail else {
			return;
		};
		for (index, trigger) in self.props.triggers.iter().enumerate() {
			if let TouchCondition::TrailCovers(per_mille) = trigger.when &&
				!self.fired[index] &&
				trail.covers(per_mille)
			{
				self.fired[index] = true;
				fired.push(trigger.action.clone());
			}
		}
	}
}

/// The cells a finger marked. Cells never clear.
struct Trail {
	columns: usize,
	rows: usize,
	cell_width: f64,
	cell_height: f64,
	radius: f64,
	cells: Vec<u8>,
	marked: usize,
	revision: u64,
}

impl Trail {
	fn new(width: f64, height: f64, radius: u64) -> Self {
		let short_cells = |short: f64, long: f64| {
			((TRAIL_LONG_SIDE_CELLS as f64 * short / long).round() as usize).max(1)
		};
		let (columns, rows) = if width >= height {
			(TRAIL_LONG_SIDE_CELLS, short_cells(height, width))
		} else {
			(short_cells(width, height), TRAIL_LONG_SIDE_CELLS)
		};
		Self {
			columns,
			rows,
			cell_width: width / columns as f64,
			cell_height: height / rows as f64,
			radius: radius as f64,
			cells: vec![0; columns * rows],
			marked: 0,
			revision: 0,
		}
	}

	/// Marks every cell whose centre lies within the radius of the segment.
	fn mark(&mut self, from: Point, to: Point) {
		let column_range = |low: f64, high: f64| {
			let first = ((low - self.radius) / self.cell_width).floor().max(0.0) as usize;
			let last = ((high + self.radius) / self.cell_width).ceil() as usize;
			first..last.min(self.columns)
		};
		let row_range = |low: f64, high: f64| {
			let first = ((low - self.radius) / self.cell_height).floor().max(0.0) as usize;
			let last = ((high + self.radius) / self.cell_height).ceil() as usize;
			first..last.min(self.rows)
		};
		let columns = column_range(from.x.min(to.x), from.x.max(to.x));
		let rows = row_range(from.y.min(to.y), from.y.max(to.y));
		let mut changed = false;
		for row in rows {
			for column in columns.clone() {
				let centre = Point {
					x: (column as f64 + 0.5) * self.cell_width,
					y: (row as f64 + 0.5) * self.cell_height,
				};
				let cell = &mut self.cells[row * self.columns + column];
				if *cell == 0 && distance_to_segment(centre, from, to) <= self.radius {
					*cell = u8::MAX;
					self.marked += 1;
					changed = true;
				}
			}
		}
		if changed {
			self.revision += 1;
		}
	}

	fn covers(&self, per_mille: u16) -> bool {
		self.marked * usize::from(PER_MILLE) >= usize::from(per_mille) * self.cells.len()
	}

	fn image(&self) -> TrailImage {
		TrailImage {
			width: self.columns as u32,
			height: self.rows as u32,
			bytes: self.cells.clone(),
			revision: self.revision,
		}
	}
}

fn distance_to_segment(point: Point, from: Point, to: Point) -> f64 {
	let (dx, dy) = (to.x - from.x, to.y - from.y);
	let length_squared = dx * dx + dy * dy;
	let along = if length_squared == 0.0 {
		0.0
	} else {
		(((point.x - from.x) * dx + (point.y - from.y) * dy) / length_squared).clamp(0.0, 1.0)
	};
	(point.x - from.x - along * dx).hypot(point.y - from.y - along * dy)
}

#[cfg(test)]
mod tests;
