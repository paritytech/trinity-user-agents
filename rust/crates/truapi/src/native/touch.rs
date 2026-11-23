//! Touch nodes for a native host: which ones take touch, and the state of each
//! one drawn.

use std::sync::Arc;

use parking_lot::Mutex;
use truapi::latest::{RendererNode, TouchProps};

use crate::materials::{TouchArea, TouchInputs, TouchPhase, TrailImage, touch_nodes};

/// One `Touch` node as drawn. Make a new one when the node's props or size
/// change, and keep it while they stay, so the trail and fired triggers stay
/// with the body.
#[derive(uniffi::Object)]
pub struct NativeTouchArea(Mutex<TouchArea>);

#[uniffi::export]
impl NativeTouchArea {
	/// A node with `props`, `width` by `height` logical pixels, before any touch.
	#[uniffi::constructor]
	pub fn new(props: TouchProps, width: f64, height: f64) -> Arc<Self> {
		Arc::new(Self(Mutex::new(TouchArea::new(props, width, height))))
	}

	/// Takes one finger sample at `x`, `y` logical pixels from the node's top
	/// left, `at_seconds` on the host's clock, and returns the actions to send,
	/// each with an empty payload.
	pub fn touch(&self, phase: TouchPhase, x: f64, y: f64, at_seconds: f64) -> Vec<String> {
		self.0.lock().touch(phase, x, y, at_seconds)
	}

	/// The touch arguments at `now_seconds` on the host's clock.
	pub fn inputs(&self, now_seconds: f64) -> TouchInputs {
		self.0.lock().inputs(now_seconds)
	}

	/// The trail, when the node keeps one.
	pub fn trail(&self) -> Option<TrailImage> {
		self.0.lock().trail()
	}
}

/// Whether each `Touch` node of `tree`, depth first, takes touch. One that does
/// not draws its children like a `Box`.
#[uniffi::export]
pub fn renderer_touch_nodes(tree: RendererNode) -> Vec<bool> {
	touch_nodes(&tree)
}
