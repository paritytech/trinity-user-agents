//! Runs a product worker compiled to wasm as the selected product.

use std::path::Path;

use anyhow::{Context, Result};
use truapi::platform::{ProductContext, ProductExecutionKind};
use truapi::{HostAdmin, WasmEnv, WasmWorker};

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
