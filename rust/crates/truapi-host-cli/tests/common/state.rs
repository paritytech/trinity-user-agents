use super::offline::offline_host_command;

/// The name the state of the stack [`offline_host_command`] runs on is kept under, as
/// `network-check` reports it: its directory under `v2`, and its network in an account store.
///
/// A stack given as a local file is a stack of its own, whatever `id` its descriptor declares,
/// so this is not paseo-next-v2.
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
