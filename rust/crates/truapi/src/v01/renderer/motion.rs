use std::collections::BTreeSet;

use super::{Drive, Keyframe, Modifier, Pan, RendererNode};

/// Pans a tree may declare; later ones are dropped.
const MAX_PANS: usize = 4;
/// Drives a tree may declare; later ones are dropped.
const MAX_DRIVES: usize = 32;
/// Keyframes one drive may carry; a drive with more is dropped.
const MAX_KEYFRAMES: usize = 16;

impl RendererNode {
	/// Drop every `Pan` and `Drive` a host must not run, so the tree draws
	/// without that motion instead of failing.
	///
	/// A pan is dropped when its id repeats an earlier one in depth-first
	/// order, when its `distance` or `period` is zero, or past the first four.
	/// A drive is dropped when it names no remaining pan, when its keyframes
	/// are empty, more than sixteen, not finite or not strictly ascending in
	/// `at`, or past the first thirty-two.
	pub fn clear_invalid_motion(&mut self) {
		let mut pans = BTreeSet::new();
		self.for_each_modifiers(&mut |modifiers| {
			modifiers.retain(|modifier| match modifier {
				Modifier::Pan(pan) => {
					pans.len() < MAX_PANS && pan.is_valid() && pans.insert(pan.id.clone())
				},
				_ => true,
			});
		});

		let mut drives = 0;
		self.for_each_modifiers(&mut |modifiers| {
			modifiers.retain(|modifier| match modifier {
				Modifier::Drive(drive) => {
					let kept =
						drives < MAX_DRIVES && pans.contains(&drive.source) && drive.is_valid();
					drives += usize::from(kept);
					kept
				},
				_ => true,
			});
		});
	}

	/// Visits the modifiers of every node, depth first, parents before
	/// children.
	fn for_each_modifiers(&mut self, visit: &mut impl FnMut(&mut Vec<Modifier>)) {
		let children = match self {
			Self::Box { modifiers, children, .. } |
			Self::Column { modifiers, children, .. } |
			Self::Row { modifiers, children, .. } |
			Self::Text { modifiers, children, .. } |
			Self::Button { modifiers, children, .. } => {
				visit(modifiers);
				children
			},
			Self::Spacer { modifiers } |
			Self::TextField { modifiers, .. } |
			Self::Image { modifiers, .. } => {
				visit(modifiers);
				return;
			},
			Self::Materials { children, .. } => children,
			Self::Nil | Self::String { .. } => return,
		};
		for child in children {
			child.for_each_modifiers(visit);
		}
	}
}

impl Pan {
	fn is_valid(&self) -> bool {
		self.distance != 0 && self.period != Some(0)
	}
}

impl Drive {
	fn is_valid(&self) -> bool {
		let finite = |bits: u32| f32::from_bits(bits).is_finite();
		(1..=MAX_KEYFRAMES).contains(&self.keyframes.len()) &&
			self.keyframes
				.iter()
				.all(|Keyframe { at, value, .. }| finite(*at) && finite(*value)) &&
			self.keyframes
				.windows(2)
				.all(|pair| f32::from_bits(pair[0].at) < f32::from_bits(pair[1].at))
	}
}

#[cfg(test)]
mod tests;
