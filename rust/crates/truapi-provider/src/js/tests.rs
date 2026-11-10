use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use super::*;

wasm_bindgen_test_configure!(run_in_browser);

/// A client whose `load` and `save` record what they were called with, so a
/// test can prove the wasm boundary actually reaches the object the host
/// passed. `load` answers `stored`, and a call without a stack records `-`.
fn recording_client(
	calls: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
	stored: String,
) -> JsValue {
	let client = js_sys::Object::new();

	let load_calls = std::rc::Rc::clone(&calls);
	let load = Closure::<dyn FnMut(String, JsValue) -> JsValue>::new(
		move |genesis: String, stack: JsValue| {
			let stack = stack.as_string().unwrap_or_else(|| "-".to_owned());
			load_calls.borrow_mut().push(format!("load {genesis} {stack}"));
			JsValue::from_str(&stored)
		},
	);
	js_sys::Reflect::set(&client, &JsValue::from_str("load"), load.as_ref()).expect("set load");
	load.forget();

	let save_calls = std::rc::Rc::clone(&calls);
	let save = Closure::<dyn FnMut(String, String)>::new(move |genesis: String, blob: String| {
		save_calls.borrow_mut().push(format!("save {genesis} {}", blob.len()));
	});
	js_sys::Reflect::set(&client, &JsValue::from_str("save"), save.as_ref()).expect("set save");
	save.forget();

	client.into()
}

#[wasm_bindgen_test]
fn connection_types_read_the_fields_a_host_names() {
	let parse =
		|source: &str| connection_types_from_js(&js_sys::JSON::parse(source).expect("valid JSON"));
	assert_eq!(
		parse(r#"{"unsecure":false}"#).ok(),
		Some(crate::ConnectionTypes { unsecure: false, ..crate::ConnectionTypes::default() })
	);
	assert_eq!(parse("{}").ok(), Some(crate::ConnectionTypes::default()));
	assert!(parse(r#"{"unsecured":false}"#).is_err());
	assert!(parse(r#"{"secure":"no"}"#).is_err());
	assert!(parse("true").is_err());
}

/// The names JS actually sees. A missing `js_name` exports the Rust name
/// instead, which the tests below would not notice because they call the
/// Rust method rather than the export.
#[wasm_bindgen_test]
fn the_exported_names_match_the_documented_ones() {
	let mut builder = ChainProviderBuilder::new();
	let handle = JsValue::from(builder.build().expect("an empty builder builds"));
	for name in ["connect", "lifecycle", "loadDatabase", "saveDatabase", "addLightSpec"] {
		let found = js_sys::Reflect::get(&handle, &JsValue::from_str(name))
			.expect("the handle is an object");
		assert!(found.is_function(), "the provider does not export `{name}`");
	}

	let builder = JsValue::from(ChainProviderBuilder::new());
	let mut expected = vec![
		"setStorage",
		"setDatabaseContent",
		"setConnectionTypes",
		"addRpcChain",
		"setStack",
		"lightSpecs",
		"addLightSpec",
	];
	if cfg!(feature = "networks") {
		expected.push("addNetwork");
	}
	for name in expected {
		let found = js_sys::Reflect::get(&builder, &JsValue::from_str(name))
			.expect("the builder is an object");
		assert!(found.is_function(), "the builder does not export `{name}`");
	}
}

/// A host reads these fields by name, so the shape the TypeScript
/// declaration promises is the one that crosses the boundary.
#[wasm_bindgen_test]
fn a_lifecycle_crosses_as_the_declared_object() {
	let state = crate::ChainLifecycle {
		phase: crate::ChainPhase::Syncing { at: 7, target: 9 },
		peers: 3,
		health: crate::ChainHealth::Stalled { reason: crate::StallReason::NoProgress },
	};
	let json = js_sys::JSON::stringify(&super::lifecycle_to_js(state))
		.expect("a plain object stringifies");
	let crossed: serde_json::Value = serde_json::from_str(&String::from(json)).expect("valid JSON");
	assert_eq!(
		crossed,
		serde_json::json!({
			"phase": { "kind": "syncing", "at": 7, "target": 9 },
			"peers": 3,
			"health": { "kind": "stalled", "reason": "noProgress" },
		})
	);
}

/// The crate stores nothing, so a provider nobody gave storage to must say
/// so rather than quietly never warming up.
#[wasm_bindgen_test]
async fn without_storage_the_database_calls_fail() {
	let genesis = format!("0x{}", "11".repeat(32));
	let mut builder = ChainProviderBuilder::new();
	builder.add_light_chain(&genesis, "{}".to_owned()).expect("the genesis parses");
	let provider = builder.build().expect("builds");

	assert!(
		provider.load_database(&genesis).await.is_err(),
		"loading needs somewhere to load from"
	);
	assert!(provider.save_database(&genesis).await.is_err(), "saving needs somewhere to save to");
}

/// The client the host passed is reached across the wasm boundary, with the genesis
/// hash it expects.
#[wasm_bindgen_test]
async fn the_host_client_is_called_with_the_genesis_hash() {
	let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
	let genesis = format!("0x{}", "22".repeat(32));

	let mut builder = ChainProviderBuilder::new();
	builder.add_light_chain(&genesis, "{}".to_owned()).expect("the genesis parses");
	builder
		.set_storage(recording_client(std::rc::Rc::clone(&calls), "stored-blob".to_owned()))
		.expect("the client has load and save");
	let provider = builder.build().expect("builds");

	assert!(
		provider.load_database(&genesis).await.expect("the client answers"),
		"the client returned a blob, so one is now in hand"
	);
	assert_eq!(calls.borrow().as_slice(), [format!("load {genesis} -")]);
}

/// A store serving more than one stack tells their databases apart by the
/// stack it is called with, and a host may hand over a chain's spec on either
/// side of `build`.
#[wasm_bindgen_test]
async fn a_stack_provider_names_its_stack_to_the_client() {
	use crate::test_stack::{
		PEOPLE_SPEC, RELAY_SPEC, STACK_ID, STACK_LINK, descriptor_bytes, genesis,
	};

	let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
	let descriptor = descriptor_bytes("one");
	let link = STACK_LINK;
	let (relay, people) = (hex0x(&genesis(RELAY_SPEC)), hex0x(&genesis(PEOPLE_SPEC)));

	let mut builder = ChainProviderBuilder::new();
	assert!(
		builder.add_light_spec(&relay, RELAY_SPEC.as_bytes()).is_err(),
		"a spec has no descriptor to be checked against before setStack"
	);
	builder
		.set_stack(link, &descriptor)
		.expect("an unsigned link opens a plain descriptor");
	assert!(builder.set_stack(link, &descriptor).is_err(), "a second stack would drop the specs");
	let listed: Vec<String> = builder.light_specs().iter().map(|spec| spec.url()).collect();
	assert_eq!(listed, ["https://stack.example/spec.json"; 3]);
	assert!(
		builder.add_light_spec(&relay, PEOPLE_SPEC.as_bytes()).is_err(),
		"People's spec is not the relay's"
	);
	builder
		.add_light_spec(&relay, RELAY_SPEC.as_bytes())
		.expect("a refused spec leaves the builder usable");
	builder
		.set_storage(recording_client(
			std::rc::Rc::clone(&calls),
			crate::storage::seal(STACK_ID, "one", "stored-blob"),
		))
		.expect("the client has load and save");
	let provider = builder.build().expect("builds");
	assert!(builder.set_stack(link, &descriptor).is_err(), "the builder was consumed");

	assert!(
		provider.load_database(&people).await.is_err(),
		"People has no spec yet, so the provider does not serve it"
	);
	provider
		.add_light_spec(&people, PEOPLE_SPEC.as_bytes())
		.expect("the spec matches the descriptor");
	assert!(provider.load_database(&people).await.expect("the client answers"));
	assert_eq!(
		calls.borrow().as_slice(),
		[format!("load {relay} {STACK_ID}"), format!("load {people} {STACK_ID}")]
	);
}

/// The first `addRpcChain` turns the catalog off. A host that mixes remote
/// nodes with bundled networks relies on that: a bundled chain it did not
/// register is refused rather than started on the light client, and one
/// that `addNetwork` registered first is still served.
///
/// `loadDatabase` refuses a chain `connect` would refuse, without starting
/// anything, so it stands in for `connect` here.
#[cfg(feature = "networks")]
#[wasm_bindgen_test]
async fn the_first_add_rpc_chain_turns_the_catalog_off() {
	let paseo = ChainProviderBuilder::new().add_network("paseo-next-v2").expect("bundled");
	let previewnet = ChainProviderBuilder::new().add_network("previewnet").expect("bundled");
	let remote_people = |builder: &mut ChainProviderBuilder| {
		builder
			.add_rpc_chain(&paseo.people(), "ws://people.invalid")
			.expect("registers")
	};

	let catalog_only = ChainProviderBuilder::new();
	let mut remote_only = ChainProviderBuilder::new();
	remote_people(&mut remote_only);
	let mut network_then_remote = ChainProviderBuilder::new();
	network_then_remote.add_network("paseo-next-v2").expect("bundled");
	remote_people(&mut network_then_remote);

	let mut served = Vec::new();
	for mut builder in [catalog_only, remote_only, network_then_remote] {
		builder
			.set_storage(recording_client(std::rc::Rc::default(), String::new()))
			.expect("the client has load and save");
		let provider = builder.build().expect("builds");
		let mut relays = Vec::new();
		for relay in [paseo.relay(), previewnet.relay()] {
			relays.push(provider.load_database(&relay).await.is_ok());
		}
		served.push(relays);
	}

	assert_eq!(served, [[true, true], [false, false], [true, false]]);
}

/// A seeded blob is used without consulting the client at all.
#[wasm_bindgen_test]
async fn a_seeded_blob_beats_the_client() {
	let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
	let genesis = format!("0x{}", "33".repeat(32));

	let mut builder = ChainProviderBuilder::new();
	builder.add_light_chain(&genesis, "{}".to_owned()).expect("the genesis parses");
	builder
		.set_storage(recording_client(std::rc::Rc::clone(&calls), "stored-blob".to_owned()))
		.expect("the client has load and save");
	builder
		.set_database_content(&genesis, "seeded-blob".to_owned())
		.expect("the genesis parses");
	let provider = builder.build().expect("builds");

	assert!(provider.load_database(&genesis).await.expect("in hand"));
	assert!(
		calls.borrow().is_empty(),
		"a seeded blob is already in hand, so the client is never asked"
	);
}
