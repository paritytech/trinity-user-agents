//! The stack a browser host runs, and the JS callbacks that refresh it.
//!
//! [`stack_from_js`] resolves `runtimeConfig.stack`, and [`start_refresh`]
//! keeps its descriptor current through the host's callbacks.

use std::sync::Arc;

use js_sys::{Array, Function, Object, Reflect, Uint8Array};
use send_wrapper::SendWrapper;
use truapi::latest::GenericError;
use wasm_bindgen::{JsCast, JsValue};

#[cfg(feature = "test-host")]
use super::get_required_bytes32_at;
use super::{
	await_optional_promise, bytes32_from_js, call_js_function, get_optional_array_at,
	get_optional_bool_at, get_optional_function, get_optional_object, get_optional_string_at,
	get_required_object, get_required_string_at, js_to_string, string_from_js,
};
#[cfg(feature = "test-host")]
use crate::platform::{StackChains, StackIdentity};
use crate::{
	Spawner,
	chain_runtime::encode_hex,
	platform::{
		CoreStorage, CoreStorageRecords, ReservedValuesPolicy, Stack, StackFetchRequest,
		StackFetchResponse, StackHost, StackMismatch, StackRefresh, StackRefreshOutcome,
		StackStatus,
	},
	time::{RealTimer, SystemClock},
};

/// The stack `runtimeConfig` names, and the refresh of its descriptor, not
/// started yet: `stack`, the pointer and descriptor copy a host build
/// bundles, or, in a test-host build only, `testStack`, built from its parts
/// because a test host has no descriptor to bundle. A test stack has no
/// refresh.
///
/// The core keeps the last descriptor it verified in `storage`, and starts
/// from it when it is newer than the bundled copy.
pub async fn stack_from_js(
	runtime_config: &JsValue,
	storage: Arc<dyn CoreStorage>,
) -> Result<(Stack, Option<StackRefresh>), JsValue> {
	#[cfg(feature = "test-host")]
	if let Some(test_stack) =
		get_optional_object(runtime_config, "testStack", "runtimeConfig.testStack")?
	{
		return Ok((test_stack_from_js(&test_stack)?, None));
	}
	let stack = get_required_object(runtime_config, "stack", "runtimeConfig.stack")?;
	refuse_unknown_fields(
		&stack,
		"runtimeConfig.stack",
		&["pointer", "descriptor", "reservedValues"],
	)?;
	let pointer = get_required_string_at(&stack, "pointer", "runtimeConfig.stack.pointer")?;
	let descriptor = bytes_at(&stack, "descriptor")?
		.ok_or_else(|| JsValue::from_str("runtimeConfig.stack.descriptor must be a Uint8Array"))?;
	let reserved_values = reserved_values_from_js(&stack, "runtimeConfig.stack.reservedValues")?;
	let refresh = StackRefresh::load(
		&pointer,
		&descriptor,
		reserved_values,
		Box::new(CoreStorageRecords::new(storage)),
		Arc::new(RealTimer),
		Box::new(SystemClock),
	)
	.await
	.map_err(|error| JsValue::from_str(&format!("runtimeConfig.stack: {error}")))?;
	Ok((refresh.stack(), Some(refresh)))
}

/// Starts keeping `refresh` current through the host's `fetchStackDescriptor`
/// callback, and reports the status to `stackChanged` at start and on each
/// change. Without `fetchStackDescriptor` the core stays on the copy it
/// started from, and `stackChanged` gets the status at start and again
/// whenever what a chain answered changes it.
pub fn start_refresh(
	refresh: &StackRefresh,
	callbacks: &JsValue,
	spawner: &Spawner,
) -> Result<(), JsValue> {
	let changed = get_optional_function(callbacks, "stackChanged")?.map(SendWrapper::new);
	let fetch = get_optional_function(callbacks, "fetchStackDescriptor")?;
	let started = match (fetch, changed) {
		(Some(fetch), changed) => {
			let host = JsStackHost { fetch: SendWrapper::new(fetch), changed };
			refresh.start(Box::new(host), spawner)
		},
		(None, Some(changed)) => refresh
			.start_reporting(Box::new(move |status| report_status(&changed, status)), spawner),
		(None, None) => return Ok(()),
	};
	debug_assert!(started, "a refresh that was just loaded has not started");
	Ok(())
}

/// The stack's status as the `stackChanged` callback receives it, or
/// `undefined` for a test stack, which has no refresh.
pub fn stack_status_to_js(refresh: Option<&StackRefresh>) -> JsValue {
	refresh.map_or(JsValue::UNDEFINED, |refresh| status_to_js(&refresh.status()))
}

/// The id the test stack `test_stack` serves the product `product_id` names
/// under. Fails for a stack the core would refuse and for a product the stack
/// does not serve.
#[cfg(feature = "test-host")]
pub fn test_stack_served_id(test_stack: &JsValue, product_id: &str) -> Result<String, JsValue> {
	let product_id = product_id
		.parse::<crate::ProductId>()
		.map_err(|error| JsValue::from_str(&error.to_string()))?;
	test_stack_from_js(test_stack)?
		.served_id(&product_id)
		.map(String::from)
		.map_err(|error| JsValue::from_str(&error.to_string()))
}

/// `runtimeConfig.testStack`: `id`, `generation`, `network`, `tld`,
/// `assetHub`, `people`, `bulletin` and the optional `relay`, each
/// `{ genesisHash }`, and the optional `reservedValues`. A test stack has no
/// descriptor, so its `sequence` is 0.
#[cfg(feature = "test-host")]
fn test_stack_from_js(stack: &JsValue) -> Result<Stack, JsValue> {
	refuse_unknown_fields(
		stack,
		"runtimeConfig.testStack",
		&[
			"id",
			"generation",
			"network",
			"tld",
			"relay",
			"assetHub",
			"people",
			"bulletin",
			"reservedValues",
		],
	)?;
	let genesis = |role: &str| {
		let path = format!("runtimeConfig.testStack.{role}");
		get_optional_object(stack, role, &path)?
			.map(|chain| {
				get_required_bytes32_at(&chain, "genesisHash", &format!("{path}.genesisHash"))
			})
			.transpose()
	};
	let required = |role: &str| {
		genesis(role)?.ok_or_else(|| {
			JsValue::from_str(&format!("runtimeConfig.testStack.{role} is required"))
		})
	};
	let chains = StackChains {
		relay: genesis("relay")?,
		asset_hub: required("assetHub")?,
		people: required("people")?,
		bulletin: required("bulletin")?,
	};
	let identity = StackIdentity {
		id: get_required_string_at(stack, "id", "runtimeConfig.testStack.id")?,
		generation: get_required_string_at(
			stack,
			"generation",
			"runtimeConfig.testStack.generation",
		)?,
	};
	Stack::new(
		identity,
		get_required_string_at(stack, "network", "runtimeConfig.testStack.network")?,
		get_required_string_at(stack, "tld", "runtimeConfig.testStack.tld")?,
		chains,
		0,
		&reserved_values_from_js(stack, "runtimeConfig.testStack.reservedValues")?,
	)
	.map_err(|error| JsValue::from_str(&format!("runtimeConfig.testStack: {error}")))
}

/// Refuses a field of the config object at `path` outside `fields`. A misspelt
/// or retired field would otherwise be read as absent, and its value ignored.
fn refuse_unknown_fields(object: &JsValue, path: &str, fields: &[&str]) -> Result<(), JsValue> {
	if Array::is_array(object) {
		return Err(JsValue::from_str(&format!("{path} must be an object")));
	}
	let unknown_field = Reflect::own_keys(object)?
		.iter()
		.filter_map(|key| key.as_string())
		.find(|key| !fields.contains(&key.as_str()));
	match unknown_field {
		Some(field) => Err(JsValue::from_str(&format!(
			"{path}.{field} is not a field; {path} takes {}",
			fields.join(", ")
		))),
		None => Ok(()),
	}
}

/// A stack config's optional `reservedValues`, at `path`. Each of its fields
/// is optional, and one it does not have is refused.
fn reserved_values_from_js(stack: &JsValue, path: &str) -> Result<ReservedValuesPolicy, JsValue> {
	let Some(reserved_values) = get_optional_object(stack, "reservedValues", path)? else {
		return Ok(ReservedValuesPolicy::default());
	};
	refuse_unknown_fields(&reserved_values, path, &["allowed", "extraGenesisHashes", "extraTlds"])?;
	Ok(ReservedValuesPolicy {
		allowed: get_optional_bool_at(&reserved_values, "allowed", &format!("{path}.allowed"))?
			.unwrap_or(false),
		extra_genesis_hashes: get_optional_array_at(
			&reserved_values,
			"extraGenesisHashes",
			&format!("{path}.extraGenesisHashes"),
			bytes32_from_js,
		)?,
		extra_tlds: get_optional_array_at(
			&reserved_values,
			"extraTlds",
			&format!("{path}.extraTlds"),
			string_from_js,
		)?,
	})
}

/// `runtimeConfig.stack.<name>` as bytes, or `None` when it is not set.
fn bytes_at(stack: &JsValue, name: &str) -> Result<Option<Vec<u8>>, JsValue> {
	let value = Reflect::get(stack, &JsValue::from_str(name))?;
	if value.is_null() || value.is_undefined() {
		return Ok(None);
	}
	value
		.dyn_into::<Uint8Array>()
		.map(|bytes| Some(bytes.to_vec()))
		.map_err(|_| JsValue::from_str(&format!("runtimeConfig.stack.{name} must be a Uint8Array")))
}

/// [`StackHost`] over the host's `fetchStackDescriptor` and `stackChanged`
/// callbacks.
struct JsStackHost {
	fetch: SendWrapper<Function>,
	changed: Option<SendWrapper<Function>>,
}

#[async_trait::async_trait]
impl StackHost for JsStackHost {
	async fn fetch_stack_descriptor(
		&self,
		request: StackFetchRequest,
	) -> Result<StackFetchResponse, GenericError> {
		let fetch = self.fetch.clone();
		let answer = SendWrapper::new(async move {
			let returned = call_js_function(&fetch, &[request_to_js(&request)])?;
			response_from_js(&await_optional_promise(returned).await?)
		});
		Ok(answer.await?)
	}

	fn stack_changed(&self, status: StackStatus) {
		if let Some(changed) = &self.changed {
			report_status(changed, &status);
		}
	}
}

/// Calls the host's `stackChanged` with `status`, logging what it throws.
fn report_status(changed: &Function, status: &StackStatus) {
	if let Err(error) = changed.call1(&JsValue::NULL, &status_to_js(status)) {
		web_sys::console::error_1(&error);
	}
}

/// A plain JS object with these properties.
fn object(properties: &[(&str, JsValue)]) -> JsValue {
	let object = Object::new();
	for (name, value) in properties {
		// Setting a property on a fresh plain object cannot fail.
		let _ = Reflect::set(&object, &JsValue::from_str(name), value);
	}
	object.into()
}

/// `{ url, ifNoneMatch?, maxBytes }`.
fn request_to_js(request: &StackFetchRequest) -> JsValue {
	let StackFetchRequest { url, if_none_match, max_bytes } = request;
	object(&[
		("url", JsValue::from_str(url)),
		("ifNoneMatch", JsValue::from(if_none_match.as_deref())),
		("maxBytes", JsValue::from(*max_bytes)),
	])
}

/// `{ status: "document", bytes, etag? }` or `{ status: "notModified" }`.
fn response_from_js(response: &JsValue) -> Result<StackFetchResponse, String> {
	const PATH: &str = "fetchStackDescriptor result";
	let string_at = |name: &str| {
		get_optional_string_at(response, name, &format!("{PATH}.{name}")).map_err(js_to_string)
	};
	let status = string_at("status")?.ok_or_else(|| format!("{PATH}.status is required"))?;
	match status.as_str() {
		"document" => {
			let bytes = Reflect::get(response, &JsValue::from_str("bytes"))
				.ok()
				.and_then(|bytes| bytes.dyn_into::<Uint8Array>().ok())
				.ok_or_else(|| format!("{PATH}.bytes must be a Uint8Array"))?;
			Ok(StackFetchResponse::Document { bytes: bytes.to_vec(), etag: string_at("etag")? })
		},
		"notModified" => Ok(StackFetchResponse::NotModified),
		other => Err(format!("{PATH}.status must be document or notModified, got {other:?}")),
	}
}

/// `{ descriptor, generation, stale, expired, restartRequired,
/// lastRefresh?, mismatches }`, where `lastRefresh` is `{ outcome, reason? }`
/// and `outcome` is `current`, `unreachable`, `refused` or `notStored`.
/// `mismatches` holds what [`mismatch_to_js`] gives for each.
fn status_to_js(status: &StackStatus) -> JsValue {
	let StackStatus {
		descriptor,
		generation,
		stale,
		expired,
		restart_required,
		last_refresh,
		mismatches,
	} = status;
	let last_refresh = last_refresh.as_ref().map_or(JsValue::UNDEFINED, |outcome| {
		let (outcome, reason) = match outcome {
			StackRefreshOutcome::Current => ("current", None),
			StackRefreshOutcome::Unreachable { reason } => ("unreachable", Some(reason.as_str())),
			StackRefreshOutcome::Refused { reason } => ("refused", Some(reason.as_str())),
			StackRefreshOutcome::NotStored { reason } => ("notStored", Some(reason.as_str())),
		};
		object(&[("outcome", JsValue::from_str(outcome)), ("reason", JsValue::from(reason))])
	});
	let mismatches: Array = mismatches.iter().map(mismatch_to_js).collect();
	object(&[
		("descriptor", JsValue::from_str(descriptor)),
		("generation", JsValue::from_str(generation)),
		("stale", JsValue::from(*stale)),
		("expired", JsValue::from(*expired)),
		("restartRequired", JsValue::from(*restart_required)),
		("lastRefresh", last_refresh),
		("mismatches", mismatches.into()),
	])
}

/// `{ kind: "genesisHash", role, named, answered }`, with both hashes as `0x`
/// hex, or `{ kind: "networkSuffix", named, answered }`.
fn mismatch_to_js(mismatch: &StackMismatch) -> JsValue {
	match mismatch {
		StackMismatch::GenesisHash { role, named, answered } => object(&[
			("kind", JsValue::from_str("genesisHash")),
			("role", JsValue::from_str(role)),
			("named", JsValue::from_str(&encode_hex(named))),
			("answered", JsValue::from_str(&encode_hex(answered))),
		]),
		StackMismatch::NetworkSuffix { named, answered } => object(&[
			("kind", JsValue::from_str("networkSuffix")),
			("named", JsValue::from_str(named)),
			("answered", JsValue::from_str(answered)),
		]),
	}
}
