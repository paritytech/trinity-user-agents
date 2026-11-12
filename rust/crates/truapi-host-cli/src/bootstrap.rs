//! Browser bridge served to products during local development.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Path the bridge script is served from on the frame endpoint.
pub const PATH: &str = "/bootstrap.js";

/// Shared browser sandbox belonging to the selected runner installation.
pub fn container_path() -> PathBuf {
	container_path_for_runner(&crate::script_runner::runner_path())
}

fn container_path_for_runner(runner: &Path) -> PathBuf {
	let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
	if runner == manifest.join("js/runner.ts") {
		return manifest.join("../../../target/dist/sandbox-assets/container.js");
	}
	runner.with_file_name("sandbox-assets").join("container.js")
}

/// Read the prebuilt sandbox without falling back to another installation.
pub fn read_container(path: &Path) -> Result<String> {
	std::fs::read_to_string(path).with_context(|| {
        format!(
            "cannot read browser sandbox {}; reinstall truapi-host, or run `make cli-runner` in a source checkout",
            path.display()
        )
    })
}

/// Install the shared sandbox synchronously before the page's product code.
pub fn script(frame_url: &str, container: &str) -> String {
	let url = serde_json::to_string(frame_url).expect("a string always serializes");
	format!("window.__truapi_localhost = {{ url: {url} }};\n{container}")
}

/// HTTP URL the bridge script is served from, for a frame endpoint that has
/// one. A private Unix socket is reachable only by the CLI's own product
/// runner, so it has no browser-facing address.
pub fn bridge_url(frame_url: &str) -> Option<String> {
	let authority = frame_url.strip_prefix("ws://")?;
	Some(format!("http://{authority}{PATH}"))
}

#[cfg(test)]
mod tests;
