//! The wasm-bindgen surface `@parity/trinity-stack` wraps. Results cross as plain JS objects of the
//! crate's own types, and errors as a `TrinityStackError` carrying the rule's [`Error::code`].

use crate::{
	Accepted, Document, Error, Fetched, Pointer, SpecFile, Verified, accept as accept_fetched,
	format, open as open_bytes,
};
use serde::Serialize;
use tsify::{Ts, Tsify};
use wasm_bindgen::prelude::*;

fn to_js<T: Tsify + Serialize>(value: &T) -> Result<Ts<T>, JsValue> {
	value.into_ts().map_err(|error| JsError::from(error).into())
}

fn throw(error: Error) -> JsValue {
	let thrown = js_sys::Error::new(&error.to_string());
	thrown.set_name("TrinityStackError");
	js_sys::Reflect::set(&thrown, &"code".into(), &error.code().into())
		.expect("setting a property on a fresh Error cannot fail");
	thrown.into()
}

fn secret_key(secret: &[u8]) -> Result<[u8; 32], JsValue> {
	secret.try_into().map_err(|_| throw(Error::SecretKeyLength(secret.len())))
}

fn json(value: serde_json::Value) -> String {
	value.to_string()
}

/// Parses a document without checking a key. `signed` says whether it is an envelope.
#[wasm_bindgen]
pub fn parse(bytes: &[u8]) -> Result<String, JsValue> {
	let document = Document::parse(bytes).map_err(throw)?;
	let signed = matches!(document, Document::Envelope { .. });
	Ok(json(serde_json::json!({ "descriptor": document.descriptor(), "signed": signed })))
}

/// Opens a document under a pointer's rules.
#[wasm_bindgen]
pub fn open(bytes: &[u8], url: &str, key: Option<String>) -> Result<Ts<Verified>, JsValue> {
	let pointer = Pointer::new(url, key.as_deref()).map_err(throw)?;
	to_js(&open_bytes(bytes, &pointer).map_err(throw)?)
}

/// Decides what a reader does with fetched bytes; `fetched` is undefined when the fetch failed.
#[wasm_bindgen]
pub fn accept(
	url: &str,
	key: Option<String>,
	stored: Option<Vec<u8>>,
	fetched: Option<Vec<u8>>,
	now_unix_seconds: f64,
) -> Result<Ts<Accepted>, JsValue> {
	let pointer = Pointer::new(url, key.as_deref()).map_err(throw)?;
	let fetched = fetched.as_deref().map_or(Fetched::Unreachable, Fetched::Document);
	to_js(
		&accept_fetched(&pointer, stored.as_deref(), fetched, now_unix_seconds as i64)
			.map_err(throw)?,
	)
}

/// The pointer a link carries.
#[wasm_bindgen(js_name = decodeLink)]
pub fn decode_link(link: &str) -> Result<Ts<Pointer>, JsValue> {
	to_js(&Pointer::from_link(link).map_err(throw)?)
}

/// The link that carries a pointer, under `base`.
#[wasm_bindgen(js_name = encodeLink)]
pub fn encode_link(url: &str, key: Option<String>, base: &str) -> Result<String, JsValue> {
	Ok(Pointer::new(url, key.as_deref()).map_err(throw)?.to_link(base))
}

/// Whether two pointers name the same stack.
#[wasm_bindgen(js_name = sameStack)]
pub fn same_stack(
	url: &str,
	key: Option<String>,
	other_url: &str,
	other_key: Option<String>,
) -> Result<bool, JsValue> {
	let pointer = Pointer::new(url, key.as_deref()).map_err(throw)?;
	let other = Pointer::new(other_url, other_key.as_deref()).map_err(throw)?;
	Ok(pointer.same_stack(&other))
}

/// The 0x-prefixed content hash of a plain descriptor: its canonical form but for `sequence`.
#[wasm_bindgen(js_name = contentHash)]
pub fn content_hash(descriptor: &[u8]) -> Result<String, JsValue> {
	Ok(format::to_hex(&crate::content_hash(descriptor).map_err(throw)?))
}

/// A plain descriptor in its canonical form: sorted keys, unknown members kept.
#[wasm_bindgen]
pub fn canonical(descriptor: &[u8]) -> Result<Vec<u8>, JsValue> {
	crate::canonical(descriptor).map_err(throw)
}

/// What identifies a pointer's stack; readers key what they store by it.
#[wasm_bindgen(js_name = stackId)]
pub fn stack_id(url: &str, key: Option<String>) -> Result<String, JsValue> {
	Ok(Pointer::new(url, key.as_deref()).map_err(throw)?.stack_id())
}

/// Wraps a plain descriptor in a signed envelope.
#[wasm_bindgen]
pub fn sign(descriptor: &[u8], secret: &[u8]) -> Result<Vec<u8>, JsValue> {
	crate::sign(descriptor, &secret_key(secret)?).map_err(throw)
}

/// The 0x-prefixed public key of a 32-byte secret key.
#[wasm_bindgen(js_name = publicKey)]
pub fn public_key(secret: &[u8]) -> Result<String, JsValue> {
	Ok(format::to_hex(&crate::public_key(&secret_key(secret)?)))
}

/// Checks a spec's bytes against its digest.
#[wasm_bindgen(js_name = checkSpec)]
pub fn check_spec(spec: Ts<SpecFile>, bytes: &[u8]) -> Result<(), JsValue> {
	let spec = spec.to_rust().map_err(JsError::from)?;
	spec.check(bytes).map_err(throw)
}

/// The 0x-prefixed genesis hash a light spec implies.
#[wasm_bindgen(js_name = lightSpecGenesis)]
pub fn light_spec_genesis(bytes: &[u8]) -> Result<String, JsValue> {
	Ok(format::to_hex(&crate::light_spec_genesis(bytes).map_err(throw)?))
}
