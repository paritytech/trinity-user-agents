use super::bundled::paseo_next_v2;

/// A local descriptor file with the bundled paseo-next-v2 id, so state paths stay those of the
/// built-in network, whose chains are on loopback ports nothing listens on. Passed as
/// `HOST_CLI_STACK`, it lets a command start without a descriptor fetch or a live chain.
pub static OFFLINE_PASEO_NEXT_V2_STACK: std::sync::LazyLock<std::path::PathBuf> =
	std::sync::LazyLock::new(|| {
		let mut descriptor = paseo_next_v2();
		for (port, chain) in (1..).zip(&mut descriptor.chains) {
			chain.rpc = vec![format!("ws://127.0.0.1:{port}")];
		}
		let path = std::env::temp_dir()
			.join(format!("truapi-host-offline-stack-{}.json", std::process::id()));
		let document = serde_json::to_vec(&descriptor).expect("descriptor does not serialize");
		std::fs::write(&path, document).expect("cannot write the offline stack");
		path
	});

/// `truapi-host` on [`OFFLINE_PASEO_NEXT_V2_STACK`].
pub fn offline_host_command() -> std::process::Command {
	let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_truapi-host"));
	command.env("HOST_CLI_STACK", &*OFFLINE_PASEO_NEXT_V2_STACK);
	command
}
