//! Runs a product worker compiled to wasm as the selected product.

use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};
use truapi::{
	HostAdmin, SigningHostRuntime, WasmEnv, WasmWorker,
	platform::{ProductContext, ProductExecutionKind},
};

use crate::chat_surface::{ChatSurface, ChatWorker};

/// The context a worker for `product_id` runs under.
pub fn context(product_id: &str) -> Result<ProductContext> {
	ProductContext::new_with_execution(product_id.to_string(), ProductExecutionKind::Worker)
		.map_err(|error| anyhow::anyhow!("invalid product id: {error}"))
}

/// Run the worker at `path` against `admin`'s product until its entry point
/// returns, passing each line it logs to `log`.
pub async fn run(
	admin: HostAdmin,
	path: &Path,
	log: impl Fn(&str) + Send + Sync + 'static,
) -> Result<()> {
	let wasm = std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
	let env = WasmEnv::for_product(admin.product_runtime().clone());
	WasmWorker::new(env, &wasm)?.with_log(log).run().await?;
	Ok(())
}

/// Run the worker at `path` as the `Worker` of `product_id` on `runtime`, and
/// attach it to `surface` until its entry point returns. The page of the
/// surface shows the rooms of the worker. What a person does on the page
/// reaches the worker through `chat.actionSubscribe()`, and the queries of the
/// page, such as a page of a room the product keeps, reach its handlers.
pub async fn run_on_surface(
	runtime: &SigningHostRuntime,
	surface: &Arc<ChatSurface>,
	product_id: &str,
	path: &Path,
	log: impl Fn(&str) + Send + Sync + 'static,
) -> Result<()> {
	let wasm = std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
	let product = context(product_id)?;
	let (worker, product_runtime) =
		WasmWorker::for_product_runtime(&wasm, |sink| runtime.product_runtime(product, sink))?;
	let product_runtime = Arc::new(product_runtime);
	let chat: Arc<dyn ChatWorker> = product_runtime.clone();
	let attachment = surface.attach_worker(&chat);
	let outcome = worker.with_log(log).run().await;
	drop(attachment);
	product_runtime.dispose();
	Ok(outcome?)
}
