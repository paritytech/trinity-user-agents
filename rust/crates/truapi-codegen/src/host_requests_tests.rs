//! Compiles the generated callers and exercises them across Rust and TypeScript.

use std::{
	fs,
	path::Path,
	process::{Command, Output},
};

use crate::{rust, rustdoc, ts};

fn run(command: &mut Command) -> Output {
	let output = command.output().expect("start integration fixture command");
	assert!(
		output.status.success(),
		"{command:?} failed: {}\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr),
	);
	output
}

fn manifest(root: &Path) -> String {
	let workspace = fs::read_to_string(root.join("Cargo.toml")).unwrap();
	let dependencies = ["async-trait", "parity-scale-codec", "futures", "hex"]
		.map(|name| workspace.lines().find(|line| line.starts_with(&format!("{name} = "))).unwrap())
		.join("\n");
	format!(
		r#"
[workspace]

[workspace.dependencies]
{dependencies}

[package]
name = "host-request-fixture"
version = "0.0.0"
edition = "2024"

[lib]
name = "truapi"

[[bin]]
name = "host-request-fixture"
required-features = ["runtime"]

[features]
runtime = ["core_runtime/runtime", "dep:futures", "dep:hex"]

[dependencies]
core_runtime = {{ package = "truapi", path = {core:?}, default-features = false }}
async-trait = {{ workspace = true }}
parity-scale-codec = {{ workspace = true, features = ["derive"] }}
futures = {{ workspace = true, optional = true }}
hex = {{ workspace = true, optional = true }}
"#,
		core = root.join("rust/crates/truapi"),
	)
}

fn caller_source(api: &rustdoc::ApiDefinition, hash: &str) -> String {
	let dispatcher = rust::generate_dispatcher(api).unwrap();
	let start = dispatcher.find("/// Call the product's `probe_answer`").unwrap();
	let body = dispatcher[start..].find('{').unwrap() + start;
	let mut depth = 0;
	let end = dispatcher[body..]
		.char_indices()
		.find_map(|(offset, character)| {
			match character {
				'{' => depth += 1,
				'}' => depth -= 1,
				_ => {},
			}
			(depth == 0).then_some(body + offset + 1)
		})
		.unwrap();
	let table = rust::generate_wire_table(api, hash).unwrap();
	let constant_start = table.find("/// Wire discriminants for `probe_answer`").unwrap();
	let constant_end = table[constant_start..].find(';').unwrap() + constant_start + 1;
	let constant = &table[constant_start..constant_end];
	let caller = &dispatcher[start..end];
	indoc::formatdoc!(
		r#"
        use std::sync::Arc;
        use crate as truapi;
        use crate::versioned;
        use core_runtime::{{subscription::HostInitiatedSubscriptionManager, transport::Transport}};

        mod wire_table {{
            use core_runtime::generated::wire_table::MethodIds;
            {constant}
        }}

        {caller}
        "#,
	)
}

#[test]
#[ignore = "requires Node, Bun and installed workspace dependencies"]
fn host_initiated_request_roundtrip() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..").canonicalize().unwrap();
	let target = root.join("target/codegen-host-requests");
	fs::create_dir_all(&target).unwrap();
	let directory = tempfile::Builder::new().prefix("fixture-").tempdir_in(&target).unwrap();
	let fixture = directory.path();
	let source = fixture.join("src");
	fs::create_dir(&source).unwrap();
	fs::write(fixture.join("Cargo.toml"), manifest(&root)).unwrap();
	fs::copy(root.join("Cargo.lock"), fixture.join("Cargo.lock")).unwrap();
	fs::write(source.join("lib.rs"), include_str!("../tests/fixtures/host_requests/lib.rs"))
		.unwrap();
	fs::write(source.join("main.rs"), include_str!("../tests/fixtures/host_requests/main.rs"))
		.unwrap();
	let nightly = fs::read_to_string(root.join("nightly-toolchain")).unwrap();
	run(Command::new("cargo")
		.arg(format!("+{}", nightly.trim()))
		.args(["rustdoc", "--offline", "--lib", "--target-dir"])
		.arg(&target)
		.args(["--", "-Z", "unstable-options", "--output-format", "json"])
		.current_dir(fixture));
	let json = fs::read_to_string(target.join("doc/truapi.json")).unwrap();
	let mut api = rustdoc::extract_api(&rustdoc::parse(&json).unwrap()).unwrap();
	let core_json = fs::read_to_string(root.join("target/doc/truapi_protocol.json"))
		.expect("run scripts/codegen.sh before the integration fixture");
	api.framework_types = rustdoc::extract_api(&rustdoc::parse(&core_json).unwrap())
		.unwrap()
		.framework_types;
	let method = &api.traits.iter().find(|service| service.name == "Probe").unwrap().methods[0];
	assert_eq!(
		(method.kind, method.wire_kind(), method.wire.host_initiated),
		(rustdoc::MethodKind::Request, rustdoc::MethodKind::Subscription, true)
	);
	let hash = ts::wire_schema_hash(&api, 2, 3).unwrap();
	fs::write(source.join("generated_caller.rs"), caller_source(&api, &hash)).unwrap();

	let generated = fixture.join("generated");
	ts::generate(&api, generated.to_str().unwrap(), 2, 3).unwrap();
	let package = root.join("js/packages/truapi/src");
	for name in ["scale", "transport"] {
		fs::write(
			fixture.join(format!("{name}.ts")),
			format!("export * from {:?};\n", package.join(format!("{name}.js")),),
		)
		.unwrap();
	}
	fs::write(
		fixture.join("runtime.ts"),
		format!("export {{ createTransport }} from {:?};\n", package.join("client.js"),),
	)
	.unwrap();
	fs::write(
		fixture.join("product.ts"),
		include_str!("../tests/fixtures/host_requests/product.ts"),
	)
	.unwrap();
	run(Command::new("node")
		.arg(root.join("node_modules/typescript/bin/tsc"))
		.args([
			"--noEmit",
			"--strict",
			"--skipLibCheck",
			"--target",
			"ES2022",
			"--module",
			"ESNext",
			"--moduleResolution",
			"bundler",
			"--types",
			"bun",
			"product.ts",
		])
		.current_dir(fixture));
	let output = run(Command::new("cargo")
		.args(["run", "--offline", "--features", "runtime", "--target-dir"])
		.arg(&target)
		.args(["--bin", "host-request-fixture", "--"])
		.arg(fixture.join("product.ts"))
		.current_dir(fixture));
	assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "host-request-roundtrip-ok");
}
