use trinity_stack::Pointer;

use crate::stack_descriptor::FETCH_TIMEOUT;

use super::{
	super::testing::{edit_descriptor, serve_stack_and_light_specs, served_etag},
	*,
};

/// A copy the CLI cannot run is a failed fetch, so the core keeps the copy it
/// holds rather than store one every later command would fail on.
#[tokio::test]
async fn a_copy_the_cli_cannot_run_is_a_failed_fetch() {
	let without_dub = edit_descriptor(|descriptor| descriptor["services"] = serde_json::json!([]));
	let (server, _) = serve_stack_and_light_specs(vec![without_dub]).await;
	let url = format!("{server}/stack.json");
	let host = CliStackHost::new(
		Pointer::new(&url, None).expect("a loopback pointer"),
		false,
		FETCH_TIMEOUT,
	);
	let request = StackFetchRequest { url, if_none_match: None, max_bytes: 1024 * 1024 };

	let refused = host.fetch_stack_descriptor(request).await.err().map(|error| error.reason);

	assert!(refused.is_some_and(|reason| reason.contains("no dub service")));
}

/// A server that has not changed the descriptor answers `304`, so an hourly
/// fetch costs no download. The host sends the `ETag` it was last given.
#[tokio::test]
async fn an_unchanged_descriptor_is_answered_not_modified() {
	let (server, _) = serve_stack_and_light_specs(vec![edit_descriptor(|_| {})]).await;
	let url = format!("{server}/stack.json");
	let host = CliStackHost::new(
		Pointer::new(&url, None).expect("a loopback pointer"),
		false,
		FETCH_TIMEOUT,
	);
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

/// The host refuses a copy without light specs only when this run is on the
/// light client, so an RPC run keeps a copy it can run.
#[tokio::test]
async fn the_host_needs_light_specs_only_on_the_light_client() {
	let without_specs = edit_descriptor(|descriptor| {
		for chain in descriptor["chains"].as_array_mut().expect("chains is an array") {
			chain.as_object_mut().expect("a chain is an object").remove("lightSpec");
		}
	});
	let (server, _) = serve_stack_and_light_specs(vec![without_specs]).await;
	let url = format!("{server}/stack.json");
	let pointer = Pointer::new(&url, None).expect("a loopback pointer");
	let fetch = |on_light_client| {
		let host = CliStackHost::new(pointer.clone(), on_light_client, FETCH_TIMEOUT);
		let request =
			StackFetchRequest { url: url.clone(), if_none_match: None, max_bytes: 1 << 20 };
		async move { host.fetch_stack_descriptor(request).await.is_ok() }
	};

	assert_eq!((fetch(false).await, fetch(true).await), (true, false));
}

/// The core says how much it reads, and the host stops there.
#[tokio::test]
async fn the_host_reads_no_more_than_the_request_allows() {
	let (server, _) = serve_stack_and_light_specs(vec![edit_descriptor(|_| {})]).await;
	let url = format!("{server}/stack.json");
	let host = CliStackHost::new(Pointer::new(&url, None).unwrap(), false, FETCH_TIMEOUT);
	let request = StackFetchRequest { url: url.clone(), if_none_match: None, max_bytes: 10 };

	let refused = host.fetch_stack_descriptor(request).await.err().map(|error| error.reason);

	assert_eq!(refused, Some(format!("{url}: larger than 10 bytes")));
}
