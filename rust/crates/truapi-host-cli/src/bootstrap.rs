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
mod tests {
	use super::*;

	#[test]
	fn bridge_url_is_offered_only_for_tcp_endpoints() {
		assert_eq!(
			bridge_url("ws://127.0.0.1:9955").as_deref(),
			Some("http://127.0.0.1:9955/bootstrap.js")
		);
		assert_eq!(bridge_url("ws+unix:/tmp/truapi/frames.sock"), None);
	}

	#[test]
	fn script_installs_the_shared_container_after_its_escaped_endpoint() {
		assert_eq!(
			script(r#"ws://x";alert(1);//"#, "installSandbox();"),
			"window.__truapi_localhost = { url: \"ws://x\\\";alert(1);//\" };\ninstallSandbox();",
		);
	}

	#[test]
	fn installed_assets_never_fall_back_to_another_runner_version() -> Result<()> {
		let installation = tempfile::tempdir()?;
		let runner = installation.path().join("versions/current/runner.js");
		let container = container_path_for_runner(&runner);
		assert_eq!(
			container,
			installation.path().join("versions/current/sandbox-assets/container.js")
		);
		let error = read_container(&container).unwrap_err().to_string();
		assert!(error.contains("reinstall truapi-host"), "{error}");
		std::fs::create_dir_all(container.parent().unwrap())?;
		std::fs::write(&container, "matching sandbox")?;
		assert_eq!(read_container(&container)?, "matching sandbox");
		Ok(())
	}

	#[test]
	fn source_dev_uses_the_packaged_build_output() {
		let runner = Path::new(env!("CARGO_MANIFEST_DIR")).join("js/runner.ts");
		assert_eq!(
			container_path_for_runner(&runner),
			Path::new(env!("CARGO_MANIFEST_DIR"))
				.join("../../../target/dist/sandbox-assets/container.js"),
		);
	}
}
