use trinity_stack::Pointer;

use super::{
	super::{
		Network,
		testing::{edit_chain, edit_descriptor, serve_stack_and_light_specs},
	},
	*,
};

/// A copy the CLI cannot run is a failed fetch, so the core keeps the copy it
/// holds rather than store one every later command would fail on.
#[tokio::test]
async fn a_copy_the_cli_cannot_run_is_a_failed_fetch() {
	let without_dub = edit_descriptor(|descriptor| descriptor["services"] = serde_json::json!([]));
	let (server, _) = serve_stack_and_light_specs(vec![without_dub]).await;
	let url = format!("{server}/stack.json");
	let host = CliStackHost::new(Pointer::new(&url, None).expect("a loopback pointer"), false);
	let request = StackFetchRequest { url, if_none_match: None, max_bytes: 1024 * 1024 };

	let refused = host.fetch_stack_descriptor(request).await.err().map(|error| error.reason);

	assert!(refused.is_some_and(|reason| reason.contains("no dub service")));
}

/// A built-in network's kept copy serves both chain backends, so one without
/// a light spec is refused over RPC too, as `stack bundle` refuses it. Any
/// other stack may run over RPC only.
#[test]
fn a_built_in_copy_without_a_light_spec_is_refused_on_either_backend() {
	let without_spec = edit_chain("Bulletin", |chain| {
		chain.as_object_mut().expect("a chain is an object").remove("lightSpec");
	});
	let (link, _) = Network::PaseoNextV2.link_and_bundle();
	let built_in = CliStackHost::new(Pointer::from_link(&link).expect("a built-in link"), false);
	let imported = Pointer::new("https://stack.test.invalid/stack.json", None).unwrap();
	let imported = CliStackHost::new(imported, false);

	assert_eq!(
		(built_in.unrunnable(&without_spec).is_some(), imported.unrunnable(&without_spec)),
		(true, None)
	);
}
