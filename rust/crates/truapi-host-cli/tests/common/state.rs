use super::offline::offline_host_command;

/// The state name of the stack [`offline_host_command`] runs on, as `network-check` reports it.
///
/// It is the stack's directory under `v2` and its network in an account store. A local file is
/// a stack of its own, so this is not paseo-next-v2, whatever `id` the descriptor declares.
pub fn offline_state_name() -> String {
	let output = offline_host_command()
		.arg("network-check")
		.env("TRUAPI_HOST_NO_UPDATE", "1")
		.output()
		.expect("run network-check");
	let routing: serde_json::Value =
		serde_json::from_slice(&output.stdout).expect("network-check prints JSON");
	routing["state_directory"].as_str().expect("a state directory").to_owned()
}
