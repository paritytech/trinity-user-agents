//! A tree's materials, resolved for a native host.

use std::sync::Arc;

use futures::StreamExt;
use truapi::latest::{HostPlatform, MaterialFailure, RenderContext, RendererNode};

use super::{
	config::native_host_platform,
	renderer::{NativeSubscription, spawn_observation},
};
use crate::{
	NodeFallback, ResolvedMaterials, ShaderLanguage, SigningHostRuntime, Spawner,
	materials::FallbackReporter,
};

/// Observer implemented by a native host to follow the resolved materials of one tree.
#[uniffi::export(callback_interface)]
pub trait NativeMaterialsObserver: Send + Sync {
	/// Deliver the materials now and after each change.
	fn on_resolved(&self, materials: ResolvedMaterials);
}

/// One tree's resolution, followed until cancelled. To resolve a new tree
/// for the same body, start its resolution before cancelling this one, so the
/// materials they share keep their fetch.
#[derive(uniffi::Object)]
pub struct NativeMaterialsResolution {
	reporter: FallbackReporter,
	subscription: Arc<NativeSubscription>,
}

#[uniffi::export]
impl NativeMaterialsResolution {
	/// Draw node `node` plain for the rest of this resolution, because its shader did
	/// not compile (`Unsupported`) or an image argument could not be used
	/// (`check_material_image`'s reason, or `NotFound`).
	pub fn report(&self, node: u32, material: Option<crate::Bytes32>, reason: MaterialFailure) {
		self.reporter.report(NodeFallback { node, material, reason });
	}

	/// Stop following the resolution, and what it fetches.
	pub fn cancel(&self) {
		self.subscription.cancel();
	}
}

/// Follows the resolved materials of `tree` in `context` for `observer`. Each node that
/// falls back reaches `failures` once.
pub fn resolve_materials(
	runtime: &SigningHostRuntime,
	tree: RendererNode,
	context: RenderContext,
	float_filterable: bool,
	observer: Arc<dyn NativeMaterialsObserver>,
	failures: Box<dyn Fn(NodeFallback) + Send + Sync>,
	spawner: &Spawner,
) -> Arc<NativeMaterialsResolution> {
	// Metal on iOS. Android draws with OpenGL ES, and so does any other host.
	let language = match native_host_platform() {
		HostPlatform::Ios => ShaderLanguage::Metal,
		_ => ShaderLanguage::GlslEs,
	};
	let mut resolution =
		runtime.resolve_materials(&tree, &context, language, float_filterable, failures);
	let reporter = resolution.reporter();
	let subscription = spawn_observation(spawner, async move {
		while let Some(materials) = resolution.next().await {
			observer.on_resolved(materials);
		}
	});
	Arc::new(NativeMaterialsResolution { reporter, subscription })
}

/// The 1024 x 1024 noise image of `seed`, the resolved materials' `noise_seed`, one byte a
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
