//! Material plans as a native host follows them.

use std::sync::Arc;

use futures::StreamExt;
use truapi::latest::{HostPlatform, MaterialFailure, RenderContext, RendererNode};

use super::{
	config::native_host_platform,
	renderer::{NativeSubscription, spawn_observation},
};
use crate::{
	MaterialPlan, PlanFailure, ShaderLanguage, SigningHostRuntime, Spawner, materials::PlanReporter,
};

/// Observer implemented by a native host to follow the plan of one tree.
#[uniffi::export(callback_interface)]
pub trait NativeMaterialPlanObserver: Send + Sync {
	/// Deliver the plan now and after each change.
	fn on_plan(&self, plan: MaterialPlan);
}

/// The plans of one tree, followed until cancelled. To plan a new tree for
/// the same body, start its plan before cancelling this one, so the
/// materials they share keep their fetch.
#[derive(uniffi::Object)]
pub struct NativeMaterialPlan {
	reporter: PlanReporter,
	subscription: Arc<NativeSubscription>,
}

#[uniffi::export]
impl NativeMaterialPlan {
	/// Draw node `node` plain for the rest of this plan, because its shader did
	/// not compile (`Unsupported`) or an image argument could not be used
	/// (`check_material_image`'s reason, or `NotFound`).
	pub fn report(&self, node: u32, material: Option<crate::Bytes32>, reason: MaterialFailure) {
		self.reporter.report(PlanFailure { node, material, reason });
	}

	/// Stop following the plan, and what it fetches.
	pub fn cancel(&self) {
		self.subscription.cancel();
	}
}

/// Follows the plan of `tree` in `context` for `observer`. Each node that
/// falls back reaches `failures` once.
pub fn observe_material_plan(
	runtime: &SigningHostRuntime,
	tree: RendererNode,
	context: RenderContext,
	float_filterable: bool,
	observer: Arc<dyn NativeMaterialPlanObserver>,
	failures: Box<dyn Fn(PlanFailure) + Send + Sync>,
	spawner: &Spawner,
) -> Arc<NativeMaterialPlan> {
	// Metal on iOS. Android draws with OpenGL ES, and so does any other host.
	let language = match native_host_platform() {
		HostPlatform::Ios => ShaderLanguage::Metal,
		_ => ShaderLanguage::GlslEs,
	};
	let mut plans = runtime.material_plan(&tree, &context, language, float_filterable, failures);
	let reporter = plans.reporter();
	let subscription = spawn_observation(spawner, async move {
		while let Some(plan) = plans.next().await {
			observer.on_plan(plan);
		}
	});
	Arc::new(NativeMaterialPlan { reporter, subscription })
}

/// The 1024 x 1024 noise image of `seed`, a plan's `noise_seed`, one byte a
/// pixel, row after row. Make it once per body and upload it as a texture.
#[uniffi::export]
pub fn material_noise(seed: u32) -> Vec<u8> {
	crate::materials::noise_image(seed)
}

/// Whether an image argument the app loaded can be drawn: PNG, JPEG or WebP,
/// at most 2048 pixels a side. `None` when it can, else the reason to report.
#[uniffi::export]
pub fn check_material_image(bytes: Vec<u8>) -> Option<MaterialFailure> {
	crate::materials::check_material_image(&bytes)
}
