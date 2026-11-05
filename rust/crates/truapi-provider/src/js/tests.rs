use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use super::*;

wasm_bindgen_test_configure!(run_in_browser);

/// A client whose `load` and `save` record what they were called with, so a
/// test can prove the wasm boundary actually reaches the object the host passed.
fn recording_client(calls: std::rc::Rc<std::cell::RefCell<Vec<String>>>) -> JsValue {
	let client = js_sys::Object::new();

	let load_calls = std::rc::Rc::clone(&calls);
	let load = Closure::<dyn FnMut(String) -> JsValue>::new(move |genesis: String| {
		load_calls.borrow_mut().push(format!("load {genesis}"));
		JsValue::from_str("stored-blob")
	});
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
	for name in ["connect", "lifecycle", "loadDatabase", "saveDatabase"] {
		let found = js_sys::Reflect::get(&handle, &JsValue::from_str(name))
			.expect("the handle is an object");
		assert!(found.is_function(), "the provider does not export `{name}`");
	}

	let builder = JsValue::from(ChainProviderBuilder::new());
	let mut expected =
		vec!["setStorage", "setDatabaseContent", "setConnectionTypes", "addRpcChain"];
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
	let mut builder = ChainProviderBuilder::new();
	let provider = builder.build().expect("an empty builder builds");
	let genesis = format!("0x{}", "11".repeat(32));

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
	builder
		.set_storage(recording_client(std::rc::Rc::clone(&calls)))
		.expect("the client has load and save");
	let provider = builder.build().expect("builds");

	assert!(
		provider.load_database(&genesis).await.expect("the client answers"),
		"the client returned a blob, so one is now in hand"
	);
	assert_eq!(calls.borrow().as_slice(), [format!("load {genesis}")]);
}

/// A seeded blob is used without consulting the client at all.
#[wasm_bindgen_test]
async fn a_seeded_blob_beats_the_client() {
	let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
	let genesis = format!("0x{}", "33".repeat(32));

	let mut builder = ChainProviderBuilder::new();
	builder
		.set_storage(recording_client(std::rc::Rc::clone(&calls)))
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
