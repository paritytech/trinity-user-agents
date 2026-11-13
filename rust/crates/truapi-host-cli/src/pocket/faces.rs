//! Pocket faces as a phone draws them: each card's render stream opens as a Worker connection
//! opens, and the materials of every face the product sends are resolved and recorded in the
//! transcript.

use std::sync::Arc;

use futures::StreamExt;
use tokio::sync::watch;
use truapi::{
	FrameSink, MaterialsResolution, NodeMaterials, ProductRuntime, ProductRuntimeControl,
	ResolvedMaterials, ShaderLanguage,
	latest::{ProductRendererRenderRequest, RenderContext},
	platform::ProductContext,
};

use super::CliPocketHost;
use crate::frame_server::ProductRuntimeFactory;

/// A [`ProductRuntimeFactory`] that renders each card the Pocket host holds as a Worker
/// connection opens, as a phone does when a card comes on screen.
pub struct PocketFaces {
	inner: Arc<dyn ProductRuntimeFactory>,
	pocket: Arc<CliPocketHost>,
}

impl PocketFaces {
	/// Wrap `inner`, rendering the cards `pocket` holds for each product.
	pub fn new(inner: Arc<dyn ProductRuntimeFactory>, pocket: Arc<CliPocketHost>) -> Arc<Self> {
		Arc::new(Self { inner, pocket })
	}
}

impl ProductRuntimeFactory for PocketFaces {
	fn product_runtime(&self, product: ProductContext, sink: Arc<dyn FrameSink>) -> ProductRuntime {
		let runtime = self.inner.product_runtime(product.clone(), sink);
		for card_id in self.pocket.cards(&product) {
			tokio::spawn(follow(runtime.control(), card_id, self.pocket.clone()));
		}
		runtime
	}

	fn connection_reset(&self) -> Option<watch::Receiver<u64>> {
		self.inner.connection_reset()
	}
}

/// Resolves the materials of each face of `card_id` until its render stream ends. A new face's
/// resolution starts before the last one is dropped, so the materials they share keep their fetch.
async fn follow(control: ProductRuntimeControl, card_id: String, pocket: Arc<CliPocketHost>) {
	let context = RenderContext::PocketCard { card_id: card_id.clone() };
	let request = ProductRendererRenderRequest { context: context.clone(), payload: Vec::new() };
	let mut faces = match control.render(request) {
		Ok(faces) => faces,
		Err(error) => {
			tracing::warn!(%error, card_id, "the card was not rendered");
			return;
		},
	};
	let mut resolution: Option<MaterialsResolution> = None;
	loop {
		tokio::select! {
			face = faces.next() => {
				let Some(Ok(face)) = face else {
					return;
				};
				match control.resolve_materials(&face, &context, ShaderLanguage::GlslEs, false) {
					Ok(resolving) => resolution = Some(resolving),
					Err(error) => {
						tracing::warn!(%error, card_id, "the face's materials were not resolved");
						return;
					},
				}
			},
			Some(materials) = next_update(&mut resolution) => pocket.record_materials(&card_id, &materials),
		}
	}
}

async fn next_update(resolution: &mut Option<MaterialsResolution>) -> Option<ResolvedMaterials> {
	match resolution {
		Some(resolution) => resolution.next().await,
		None => std::future::pending().await,
	}
}

impl CliPocketHost {
	fn record_materials(&self, card_id: &str, materials: &ResolvedMaterials) {
		let nodes: Vec<_> = materials
			.nodes
			.iter()
			.map(|node| match node {
				NodeMaterials::Loading => serde_json::json!({ "state": "loading" }),
				NodeMaterials::Ready { .. } => serde_json::json!({ "state": "ready" }),
				NodeMaterials::Plain { reason } => {
					serde_json::json!({ "state": "plain", "reason": format!("{reason:?}") })
				},
			})
			.collect();
		self.transcript.record(serde_json::json!({
			"kind": "materials",
			"cardId": card_id,
			"nodes": nodes,
		}));
	}
}
