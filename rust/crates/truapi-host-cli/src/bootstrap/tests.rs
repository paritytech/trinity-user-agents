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
	assert_eq!(container, installation.path().join("versions/current/sandbox-assets/container.js"));
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
