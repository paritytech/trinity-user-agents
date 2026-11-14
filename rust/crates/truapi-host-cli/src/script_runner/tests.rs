use super::*;

#[tokio::test]
#[ignore = "requires Bun; run with --include-ignored"]
async fn caller_configuration_cannot_execute_before_the_sandbox() -> Result<()> {
	const TEST_FIXTURE: &str = "TRUAPI_BUN_LAUNCHER_TEST_FIXTURE";
	if let Some(fixture) = std::env::var_os(TEST_FIXTURE) {
		let fixture = PathBuf::from(fixture);
		let product = std::env::current_dir()?;
		let output = bun_command(&fixture.join("trusted/entry.ts"), &product)?
			.env_remove("TRUAPI_UNTRUSTED_DOTENV")
			.output()
			.await?;
		assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
		assert_eq!(String::from_utf8(output.stderr)?, "");
		assert_eq!(
			serde_json::from_slice::<serde_json::Value>(&output.stdout)?,
			serde_json::json!({
				"preloaded": false,
				"environment": null,
				"dependency": "trusted dependency",
				"caller": product,
			})
		);
		return Ok(());
	}

	let fixture = tempfile::tempdir()?;
	let product = fixture.path().join("product");
	let trusted = fixture.path().join("trusted");
	fs::create_dir_all(&product)?;
	fs::create_dir_all(trusted.join("node_modules/trusted-dependency"))?;
	fs::write(product.join("bunfig.toml"), "preload = [\"./preload.ts\"]\n")?;
	fs::write(
		product.join("preload.ts"),
		"globalThis.preloadExecuted = true; console.log('untrusted preload executed');\n",
	)?;
	fs::write(product.join(".env"), "TRUAPI_UNTRUSTED_DOTENV=loaded\n")?;
	fs::write(product.join("tsconfig.json"), "not valid JSON")?;
	fs::write(
		trusted.join("node_modules/trusted-dependency/package.json"),
		r#"{"name":"trusted-dependency","main":"index.js"}"#,
	)?;
	fs::write(
		trusted.join("node_modules/trusted-dependency/index.js"),
		"export const value = 'trusted dependency';\n",
	)?;
	let entrypoint = trusted.join("entry.ts");
	fs::write(
		&entrypoint,
		r#"import { value } from 'trusted-dependency';
console.log(JSON.stringify({
  preloaded: globalThis.preloadExecuted === true,
  environment: process.env.TRUAPI_UNTRUSTED_DOTENV ?? null,
  dependency: value,
  caller: process.env.TRUAPI_SCRIPT_CWD,
}));
"#,
	)?;
	let output = Command::new(std::env::current_exe()?)
		.args([
			"--ignored",
			"--exact",
			"script_runner::tests::caller_configuration_cannot_execute_before_the_sandbox",
		])
		.current_dir(&product)
		.env(TEST_FIXTURE, fixture.path())
		.output()
		.await?;
	assert!(
		output.status.success(),
		"{}\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	Ok(())
}

/// The override exists so a packaged install can be pointed at a working
/// copy; it has to win over the bundle sitting next to the binary.
#[test]
fn an_explicit_runner_overrides_the_packaged_bundle() {
	let install = tempfile::tempdir().unwrap();
	let executable = install.path().join("truapi-host");
	fs::write(install.path().join(PACKAGED_RUNNER), "packaged").unwrap();

	assert_eq!(
		resolve_runner(Some(OsString::from("/somewhere/custom.ts")), Some(&executable)),
		Path::new("/somewhere/custom.ts")
	);
}

/// What makes a downloaded binary able to run product scripts at all.
#[test]
fn a_bundle_beside_the_binary_is_preferred_over_the_checkout() {
	let install = tempfile::tempdir().unwrap();
	let executable = install.path().join("truapi-host");
	let bundle = install.path().join(PACKAGED_RUNNER);
	fs::write(&executable, "binary").unwrap();
	fs::write(&bundle, "packaged").unwrap();

	assert_eq!(resolve_runner(None, Some(&executable)), bundle.canonicalize().unwrap());
}

#[cfg(unix)]
#[test]
fn the_packaged_bundle_tracks_the_running_version_through_installer_symlinks() -> Result<()> {
	use std::os::unix::fs::symlink;

	let home = tempfile::tempdir()?;
	let root = home.path().join("share/truapi-host");
	let version = root.join("versions").join(env!("CARGO_PKG_VERSION"));
	let bin = home.path().join("bin");
	fs::create_dir_all(&version)?;
	fs::create_dir_all(&bin)?;

	let installed_executable = version.join("truapi-host");
	let installed_bundle = version.join(PACKAGED_RUNNER);
	fs::write(&installed_executable, "binary")?;
	fs::write(&installed_bundle, "packaged")?;
	let current = root.join("current");
	symlink(Path::new("versions").join(env!("CARGO_PKG_VERSION")), &current)?;

	let entrypoint = bin.join("truapi-host");
	symlink(root.join("current/truapi-host"), &entrypoint)?;

	let expected = installed_bundle.canonicalize()?;
	assert_eq!(resolve_runner(None, Some(&entrypoint)), expected);

	let next_version = root.join("versions/next");
	fs::create_dir_all(&next_version)?;
	fs::write(next_version.join("truapi-host"), "next binary")?;
	fs::write(next_version.join(PACKAGED_RUNNER), "next runner")?;
	fs::remove_file(&current)?;
	symlink("versions/next", current)?;

	assert_eq!(
		resolve_runner(None, Some(&entrypoint)),
		expected,
		"a running binary keeps using its matching runner after current moves"
	);
	Ok(())
}

#[test]
fn a_source_build_falls_back_to_the_checkout_runner() {
	let install = tempfile::tempdir().unwrap();
	let executable = install.path().join("truapi-host");

	assert_eq!(
		resolve_runner(None, Some(&executable)),
		Path::new(env!("CARGO_MANIFEST_DIR")).join("js/runner.ts")
	);
}

#[test]
fn an_incomplete_managed_install_does_not_fall_back_to_source_code() -> Result<()> {
	let install = tempfile::tempdir()?;
	let version = install.path().join("versions").join(env!("CARGO_PKG_VERSION"));
	fs::create_dir_all(&version)?;
	let executable = version.join("truapi-host");
	fs::write(&executable, "binary")?;
	assert_eq!(
		resolve_runner(None, Some(&executable)),
		version.canonicalize()?.join(PACKAGED_RUNNER)
	);
	Ok(())
}

#[test]
fn host_scripts_are_run_by_bun() -> Result<()> {
	let temporary = tempfile::tempdir()?;
	let script = temporary.path().join("script.ts");
	fs::write(&script, "console.log('hello');\n")?;

	let runner = runner_path().canonicalize()?;
	let command = command(
		"ws://127.0.0.1:1234",
		&ProductId::for_test("example.dot"),
		&script,
		ScriptHostRole::SigningHost,
	)?;
	let command = command.as_std();
	assert_eq!(command.get_program(), std::ffi::OsStr::new("bun"));
	assert_eq!(
		command.get_args().collect::<Vec<_>>(),
		[
			std::ffi::OsStr::new(EMPTY_BUN_CONFIG),
			std::ffi::OsStr::new("--no-env-file"),
			std::ffi::OsStr::new("--no-macros"),
			std::ffi::OsStr::new("--install=fallback"),
			std::ffi::OsStr::new("run"),
			runner.as_os_str(),
		]
	);
	assert_eq!(command.get_current_dir(), runner.parent());
	assert_eq!(
		command
			.get_envs()
			.find_map(|(key, value)| { (key == "TRUAPI_CLI_HOST_ROLE").then_some(value) }),
		Some(Some(std::ffi::OsStr::new("signing-host")))
	);
	Ok(())
}

#[test]
fn relative_runner_overrides_work_in_managed_projects() -> Result<()> {
	if let Some(script) = std::env::var_os("TRUAPI_TEST_SCRIPT") {
		let command = command(
			"ws://127.0.0.1:1234",
			&ProductId::for_test("example.dot"),
			Path::new(&script),
			ScriptHostRole::PairingHost,
		)?;
		let runner = command.as_std().get_args().last().unwrap();
		assert_eq!(Path::new(runner), fs::canonicalize("runner.js")?);
		return Ok(());
	}
	let temporary = tempfile::tempdir()?;
	fs::write(temporary.path().join("runner.js"), "runner")?;
	let project = temporary.path().join("project");
	fs::create_dir(&project)?;
	fs::write(project.join("package.json"), r#"{"truapiHost":{"script":"script.ts"}}"#)?;
	let script = project.join("script.ts");
	fs::write(&script, "script")?;
	let output = std::process::Command::new(std::env::current_exe()?)
		.args([
			"--exact",
			"script_runner::tests::relative_runner_overrides_work_in_managed_projects",
		])
		.current_dir(temporary.path())
		.env("TRUAPI_HOST_RUNNER", "runner.js")
		.env("TRUAPI_TEST_SCRIPT", script)
		.output()?;
	assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
	Ok(())
}

#[test]
fn managed_scripts_use_their_project_directory_when_selected_explicitly() -> Result<()> {
	let project = tempfile::tempdir()?;
	fs::write(project.path().join("package.json"), r#"{"truapiHost":{"script":"script.ts"}}"#)?;
	let script = project.path().join("script.ts");
	fs::write(&script, "console.log('hello');\n")?;

	let command = command(
		"ws://127.0.0.1:1234",
		&ProductId::for_test("example.dot"),
		&script,
		ScriptHostRole::SigningHost,
	)?;

	assert_eq!(
		command
			.as_std()
			.get_envs()
			.find_map(|(key, value)| { (key == "TRUAPI_SCRIPT_CWD").then_some(value) }),
		Some(Some(project.path().as_os_str()))
	);
	Ok(())
}

#[test]
fn editor_command_accepts_quoted_arguments_without_a_shell() -> Result<()> {
	let (program, arguments) = parse_editor("code --wait \"profile one\"")?;

	assert_eq!(program, "code");
	assert_eq!(arguments, ["--wait", "profile one"]);
	Ok(())
}

/// The `truapi-host` binary of the profile this test binary was built in,
/// which confines the worker. Cargo builds it for the crate's integration
/// tests.
fn launcher() -> Result<PathBuf> {
	let test = std::env::current_exe()?;
	test.ancestors()
		.skip(1)
		.map(|directory| directory.join("truapi-host"))
		.find(|binary| binary.is_file())
		.with_context(|| format!("build truapi-host first: none above {}", test.display()))
}

#[tokio::test]
#[ignore = "requires Bun and the truapi-host binary; run with --include-ignored"]
async fn a_published_worker_reads_only_its_archive_spawns_nothing_and_sees_no_home() -> Result<()> {
	let secret = tempfile::tempdir()?;
	let secret_file = secret.path().join("secret.txt");
	fs::write(&secret_file, "not for workers")?;
	let runner = tempfile::tempdir()?;
	fs::write(runner.path().join("runner.js"), "await import(process.env.TRUAPI_SCRIPT);\n")?;
	let archive = tempfile::tempdir()?;
	let script = archive.path().join("index.js");
	fs::write(
		&script,
		format!(
			r#"import fs from "node:fs";
import {{ spawnSync }} from "node:child_process";
const attempt = (action) => {{
  try {{
    action();
    return "allowed";
  }} catch {{
    return "refused";
  }}
}};
console.log(JSON.stringify({{
  ownArchive: attempt(() => fs.readFileSync(new URL("./index.js", import.meta.url))),
  scratch: attempt(() => fs.writeFileSync(`${{process.env.TMPDIR}}/note`, "x")),
  outsideFile: attempt(() => fs.readFileSync({secret:?})),
  childProcess: attempt(() => {{
    const result = spawnSync("/bin/sh", ["-c", "exit 0"]);
    if (result.error || result.status !== 0) throw result.error ?? new Error("failed");
  }}),
  bunSpawn: attempt(() => {{
    if (Bun.spawnSync(["/bin/sh", "-c", "exit 0"]).exitCode !== 0) throw new Error("failed");
  }}),
  home: process.env.HOME === undefined ? "refused" : "allowed",
}}));
"#,
			secret = secret_file.to_string_lossy(),
		),
	)?;
	assert!(std::env::var_os("HOME").is_some(), "the test needs a HOME to hide");
	let frames = tempfile::tempdir()?;
	let frame_url = format!("ws+unix:{}", frames.path().join("frames.sock").display());
	let product_id: ProductId = "fundtest.dot".parse()?;
	let worker = ConfinedWorker {
		frame_url: &frame_url,
		product_id: &product_id,
		script: &script,
		archive: archive.path(),
	};
	let (mut command, _scratch) =
		confined_command(&launcher()?, &runner.path().join("runner.js"), worker)?;
	let output = command.stdin(Stdio::null()).output().await?;
	let stdout = String::from_utf8_lossy(&output.stdout);
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(output.status.success(), "{stdout}\n{stderr}");
	let outcome: serde_json::Value = serde_json::from_str(stdout.trim())
		.with_context(|| format!("worker output: {stdout}\n{stderr}"))?;
	assert_eq!(
		outcome,
		serde_json::json!({
			"ownArchive": "allowed",
			"scratch": "allowed",
			"outsideFile": "refused",
			"childProcess": "refused",
			"bunSpawn": "refused",
			"home": "refused",
		})
	);
	Ok(())
}
