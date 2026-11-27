use trinity_stack::Pointer;

use super::{
	super::{
		Network,
		testing::{edit_chain, edit_descriptor, serve_stack_and_light_specs, served_etag},
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

/// The light client needs a light spec for each chain it runs, so a copy
/// without one is refused there. Over RPC it runs, and a light-client
/// command then fails on it.
#[test]
fn a_copy_without_a_light_spec_is_refused_only_on_the_light_client() {
	let without_spec = edit_chain("Bulletin", |chain| {
		chain.as_object_mut().expect("a chain is an object").remove("lightSpec");
	});
	let (link, _) = Network::PaseoNextV2.link_and_bundle();
	let pointer = Pointer::from_link(&link).expect("a built-in link");
	let on_light_client = CliStackHost::new(pointer.clone(), true);
	let over_rpc = CliStackHost::new(pointer, false);

	assert_eq!(
		(on_light_client.unrunnable(&without_spec).is_some(), over_rpc.unrunnable(&without_spec)),
		(true, None)
	);
}

/// A server that has not changed the descriptor answers `304`, so an hourly
/// fetch costs no download. The host sends the `ETag` it was last given.
#[tokio::test]
async fn an_unchanged_descriptor_is_answered_not_modified() {
	let (server, _) = serve_stack_and_light_specs(vec![edit_descriptor(|_| {})]).await;
	let url = format!("{server}/stack.json");
	let host = CliStackHost::new(Pointer::new(&url, None).expect("a loopback pointer"), false);
	let request = |if_none_match: Option<String>| StackFetchRequest {
		url: url.clone(),
		if_none_match,
		max_bytes: 1024 * 1024,
	};

	let Ok(StackFetchResponse::Document { etag, .. }) =
		host.fetch_stack_descriptor(request(None)).await
	else {
		panic!("the first fetch gets the document");
	};
	let again = host.fetch_stack_descriptor(request(etag.clone())).await;

	assert_eq!(
		(etag, matches!(again, Ok(StackFetchResponse::NotModified))),
		(Some(served_etag(0)), true)
	);
}
