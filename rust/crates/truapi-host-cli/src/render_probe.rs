//! Renderer probe for Worker executions, enabled by `TRUAPI_RENDER_PROBE=<path>`.
//!
//! The CLI has no native surface that draws product-rendered bodies, so a
//! product's `renderer.render` is never opened here. With the probe set, the
//! first `Custom` chat message a product posts opens that stream for it, as a
//! chat screen would. Every tree the product streams is appended to the
//! transcript at `<path>`, and after the first tree the probe publishes one
//! `bump` action into the product's renderer action stream, so the next tree
//! shows whether the product reacted.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use tokio::sync::watch;
use truapi::latest::{
    HostRendererActionSubscribeItem, ProductRendererRenderRequest, RenderContext,
};
use truapi::platform::ProductContext;
use truapi::{FrameSink, ProductRuntime, ProductRuntimeControl};

use crate::frame_server::ProductRuntimeFactory;

/// The action the probe publishes after the first tree.
pub const BUMP_ACTION: &str = "bump";

/// Opens one render stream per product connection and records what comes back.
pub struct RenderProbe {
    control: Mutex<Option<ProductRuntimeControl>>,
    transcript: PathBuf,
}

impl RenderProbe {
    /// Build the probe when `TRUAPI_RENDER_PROBE` names a transcript path.
    /// The file is truncated so a run never reads an earlier run's trees.
    pub fn from_env() -> Option<Arc<Self>> {
        let transcript = PathBuf::from(std::env::var_os("TRUAPI_RENDER_PROBE")?);
        if let Err(error) = std::fs::write(&transcript, b"") {
            tracing::warn!(?transcript, %error, "render probe transcript could not be truncated");
        }
        Some(Arc::new(Self {
            control: Mutex::new(None),
            transcript,
        }))
    }

    /// Wrap `inner` so the probe holds the control of every product connection
    /// it serves.
    pub fn tap(
        self: &Arc<Self>,
        inner: Arc<dyn ProductRuntimeFactory>,
    ) -> Arc<dyn ProductRuntimeFactory> {
        Arc::new(ProbeTappedRuntime {
            inner,
            probe: self.clone(),
        })
    }

    /// A product posted a `Custom` message: open its render stream.
    pub fn on_custom_message(&self, room_id: String, message_id: String, message_type: String) {
        let control = self
            .control
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let Some(control) = control else {
            self.record(serde_json::json!({
                "kind": "probe",
                "error": "no product connection to render on",
                "messageId": message_id,
            }));
            return;
        };
        let context = RenderContext::ChatMessage {
            room_id,
            message_id: message_id.clone(),
            message_type,
        };
        let stream = control.render(ProductRendererRenderRequest {
            context: context.clone(),
            payload: Vec::new(),
        });
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                self.record(serde_json::json!({
                    "kind": "probe",
                    "error": format!("render refused: {error}"),
                    "messageId": message_id,
                }));
                return;
            }
        };
        self.record(serde_json::json!({ "kind": "render-opened", "messageId": message_id }));
        let transcript = self.transcript.clone();
        tokio::spawn(async move {
            let mut trees = 0u32;
            while let Some(item) = stream.next().await {
                match item {
                    Ok(node) => {
                        trees += 1;
                        append(
                            &transcript,
                            serde_json::json!({
                                "kind": "tree",
                                "messageId": message_id,
                                "n": trees,
                                "node": format!("{node:?}"),
                            }),
                        );
                        if trees == 1 {
                            let published =
                                control.publish_renderer_action(HostRendererActionSubscribeItem {
                                    context: context.clone(),
                                    action_id: BUMP_ACTION.to_string(),
                                    payload: Vec::new(),
                                });
                            append(
                                &transcript,
                                serde_json::json!({
                                    "kind": "action-published",
                                    "messageId": message_id,
                                    "actionId": BUMP_ACTION,
                                    "error": published.err().map(|error| error.to_string()),
                                }),
                            );
                        }
                    }
                    Err(error) => {
                        append(
                            &transcript,
                            serde_json::json!({
                                "kind": "render-interrupted",
                                "messageId": message_id,
                                "error": format!("{error:?}"),
                            }),
                        );
                        break;
                    }
                }
            }
            append(
                &transcript,
                serde_json::json!({ "kind": "render-closed", "messageId": message_id, "trees": trees }),
            );
        });
    }

    fn attach(&self, control: ProductRuntimeControl) {
        *self
            .control
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(control);
    }

    fn record(&self, line: serde_json::Value) {
        append(&self.transcript, line);
    }
}

fn append(path: &PathBuf, line: serde_json::Value) {
    let appended = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| writeln!(file, "{line}"));
    if let Err(error) = appended {
        tracing::warn!(?path, %error, "render probe transcript could not be appended to");
    }
}

/// A [`ProductRuntimeFactory`] that hands the probe each connection's control.
struct ProbeTappedRuntime {
    inner: Arc<dyn ProductRuntimeFactory>,
    probe: Arc<RenderProbe>,
}

impl ProductRuntimeFactory for ProbeTappedRuntime {
    fn product_runtime(&self, product: ProductContext, sink: Arc<dyn FrameSink>) -> ProductRuntime {
        let runtime = self.inner.product_runtime(product, sink);
        self.probe.attach(runtime.control());
        runtime
    }

    fn connection_reset(&self) -> Option<watch::Receiver<u64>> {
        self.inner.connection_reset()
    }
}
