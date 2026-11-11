//! The stack a browser host runs, and the JS callbacks that refresh it.
//!
//! [`stack_from_js`] resolves `runtimeConfig.stack` and, when the host passes
//! `fetchStackDescriptor`, starts refreshing the descriptor through it.

use std::sync::Arc;

use js_sys::{Function, Object, Reflect, Uint8Array};
use send_wrapper::SendWrapper;
use truapi::latest::GenericError;
use wasm_bindgen::{JsCast, JsValue};

use super::{
	await_optional_promise, call_js_function, get_optional_bool_at, get_optional_function,
	get_optional_string_at, get_required_object, get_required_string_at, invoke_unit,
};
use crate::{
	Spawner,
	platform::{
		Stack, StackFetchRequest, StackFetchResponse, StackHost, StackRecords, StackRefresh,
		StackRefreshOutcome, StackStatus,
	},
	time::RealTimer,
};

/// The stack a runtime was built on, and its refresh when the stack came
/// from a descriptor.
pub struct WasmStack {
	/// The stack the runtime runs.
	pub stack: Stack,
	/// The refresh of the stack's descriptor. A test stack built from its
	/// parts has no descriptor to refresh.
	pub refresh: Option<StackRefresh>,
}

impl WasmStack {
	/// The stack's status as the `stackChanged` callback receives it, or
	/// `undefined` for a test stack.
	pub fn status(&self) -> JsValue {
		self.refresh
			.as_ref()
			.map_or(JsValue::UNDEFINED, |refresh| status_to_js(&refresh.status()))
	}

	/// Fetches the descriptor now. Does nothing for a test stack or a host
	/// without `fetchStackDescriptor`.
	pub fn refresh_now(&self) {
		if let Some(refresh) = &self.refresh {
			refresh.refresh_now();
		}
	}
}

/// The stack `runtimeConfig` names: `stack`, the pointer and descriptor copy a
/// host build bundles, or, in a test-host build only, `testStack`, built from
/// its parts because a test host has no descriptor to bundle.
///
/// `stack.stored` is the record the host last received through
/// `storeStackRecord`. The core starts from it when it is newer than the
/// bundled copy. With a `fetchStackDescriptor` callback the core refreshes the
/// descriptor through it and reports each change to `stackChanged`.
pub fn stack_from_js(
	runtime_config: &JsValue,
	callbacks: &JsValue,
	spawner: &Spawner,
) -> Result<WasmStack, JsValue> {
	#[cfg(feature = "test-host")]
	if let Some(test_stack) =
		super::get_optional_object(runtime_config, "testStack", "runtimeConfig.testStack")?
	{
		return Ok(WasmStack { stack: super::test_stack_from_js(&test_stack)?, refresh: None });
	}
	let stack = get_required_object(runtime_config, "stack", "runtimeConfig.stack")?;
	let pointer = get_required_string_at(&stack, "pointer", "runtimeConfig.stack.pointer")?;
	let descriptor = bytes_at(&stack, "descriptor")?
		.ok_or_else(|| JsValue::from_str("runtimeConfig.stack.descriptor must be a Uint8Array"))?;
	let allows_reserved_values = get_optional_bool_at(
		&stack,
		"allowsReservedValues",
		"runtimeConfig.stack.allowsReservedValues",
	)?
	.unwrap_or(false);
	let records = JsStackRecords {
		stored: bytes_at(&stack, "stored")?,
		store: get_optional_function(callbacks, "storeStackRecord")?.map(SendWrapper::new),
	};
	let refresh = StackRefresh::load(
		&pointer,
		&descriptor,
		allows_reserved_values,
		Box::new(records),
		Arc::new(RealTimer),
	)
	.map_err(|error| JsValue::from_str(&format!("runtimeConfig.stack: {error}")))?;
	if let Some(fetch) = get_optional_function(callbacks, "fetchStackDescriptor")? {
		let host = JsStackHost {
			fetch: SendWrapper::new(fetch),
			changed: get_optional_function(callbacks, "stackChanged")?.map(SendWrapper::new),
		};
		refresh.start(Box::new(host), spawner);
	}
	Ok(WasmStack { stack: refresh.stack(), refresh: Some(refresh) })
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

/// The stack's record, kept by the JS host: read from `runtimeConfig.stack`
/// and written through `storeStackRecord`.
struct JsStackRecords {
	stored: Option<Vec<u8>>,
	store: Option<SendWrapper<Function>>,
}

#[async_trait::async_trait]
impl StackRecords for JsStackRecords {
	fn read_stack_record(&self, _stack_id: &str) -> Result<Option<Vec<u8>>, GenericError> {
		Ok(self.stored.clone())
	}

	async fn write_stack_record(
		&self,
		_stack_id: &str,
		record: Vec<u8>,
	) -> Result<(), GenericError> {
		let Some(store) = &self.store else { return Ok(()) };
		Ok(invoke_unit(store, vec![Uint8Array::from(record.as_slice()).into()]).await?)
	}
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
		let Some(changed) = &self.changed else { return };
		if let Err(error) = changed.call1(&JsValue::NULL, &status_to_js(&status)) {
			web_sys::console::error_1(&error);
		}
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
		get_optional_string_at(response, name, &format!("{PATH}.{name}"))
			.map_err(super::js_to_string)
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

/// `{ descriptor, stale, expired, restartRequired, newGeneration,
/// lastRefresh? }`, where `lastRefresh` is `{ outcome, reason? }` and
/// `outcome` is `current`, `unreachable` or `refused`.
fn status_to_js(status: &StackStatus) -> JsValue {
	let StackStatus { descriptor, stale, expired, restart_required, new_generation, last_refresh } =
		status;
	let last_refresh = last_refresh.as_ref().map_or(JsValue::UNDEFINED, |outcome| {
		let (outcome, reason) = match outcome {
			StackRefreshOutcome::Current => ("current", None),
			StackRefreshOutcome::Unreachable => ("unreachable", None),
			StackRefreshOutcome::Refused { reason } => ("refused", Some(reason.as_str())),
		};
		object(&[("outcome", JsValue::from_str(outcome)), ("reason", JsValue::from(reason))])
	});
	object(&[
		("descriptor", JsValue::from_str(descriptor)),
		("stale", JsValue::from(*stale)),
		("expired", JsValue::from(*expired)),
		("restartRequired", JsValue::from(*restart_required)),
		("newGeneration", JsValue::from(*new_generation)),
		("lastRefresh", last_refresh),
	])
}
