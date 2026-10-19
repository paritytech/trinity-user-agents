//! The wasm-bindgen surface `@parity/trinity-stack` wraps. Values cross as plain JS objects of the
//! crate's own types, and errors as a `TrinityStackError` carrying the rule's [`Error::code`].

use crate::{
	Accepted, DEFAULT_LINK_BASE, Document, Error, Fetched, Pointer, SpecFile, Verified,
	accept as accept_fetched, format, open as open_bytes,
};
use serde::Serialize;
use tsify::{Ts, Tsify};
use wasm_bindgen::prelude::*;

fn throw(error: Error) -> JsValue {
	let thrown = js_sys::Error::new(&error.to_string());
	thrown.set_name("TrinityStackError");
	js_sys::Reflect::set(&thrown, &"code".into(), &error.code().into())
		.expect("setting a property on a fresh Error cannot fail");
	thrown.into()
}

/// A wrongly shaped argument is the caller's bug, not a rule a descriptor broke.
fn type_error(message: &str) -> JsValue {
	js_sys::TypeError::new(message).into()
}

fn to_js<T: Tsify + Serialize>(value: &T) -> Result<Ts<T>, JsValue> {
	value.into_ts().map_err(|error| JsError::from(error).into())
}

fn string_field(object: &JsValue, name: &str) -> Result<Option<String>, JsValue> {
	let value = js_sys::Reflect::get(object, &name.into())?;
	if value.is_undefined() {
		return Ok(None);
	}
	value
		.as_string()
		.map(Some)
		.ok_or_else(|| type_error(&format!("{name} must be a string")))
}

/// Reads a pointer through [`Pointer::new`], so a pointer that breaks a rule throws its code.
fn pointer(value: &Ts<Pointer>) -> Result<Pointer, JsValue> {
	let value = value.js_value();
	let url = string_field(&value, "url")?.ok_or_else(|| type_error("a pointer needs a url"))?;
	Pointer::new(&url, string_field(&value, "key")?.as_deref()).map_err(throw)
}

fn secret_key(secret: &[u8]) -> Result<[u8; 32], JsValue> {
	secret.try_into().map_err(|_| throw(Error::SecretKeyLength(secret.len())))
}

/// Parses a document without checking a key, as JSON `{ descriptor, signed }`; `signed` says
/// whether it is an envelope.
#[wasm_bindgen]
pub fn parse(bytes: &[u8]) -> Result<String, JsValue> {
	let document = Document::parse(bytes).map_err(throw)?;
	let signed = matches!(document, Document::Envelope { .. });
	Ok(serde_json::json!({ "descriptor": document.descriptor(), "signed": signed }).to_string())
}

/// Opens a document under a pointer's rules.
#[wasm_bindgen]
pub fn open(bytes: &[u8], pointer: Ts<Pointer>) -> Result<Ts<Verified>, JsValue> {
	to_js(&open_bytes(bytes, &self::pointer(&pointer)?).map_err(throw)?)
}

/// Decides what a reader does with fetched bytes; `fetched` is undefined when the fetch failed.
#[wasm_bindgen]
pub fn accept(
	pointer: Ts<Pointer>,
	stored: Option<Vec<u8>>,
	fetched: Option<Vec<u8>>,
	now_unix_seconds: f64,
) -> Result<Ts<Accepted>, JsValue> {
	let pointer = self::pointer(&pointer)?;
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

/// The link that carries a pointer, under `base`, which defaults to the Polkadot app's.
#[wasm_bindgen(js_name = encodeLink)]
pub fn encode_link(pointer: Ts<Pointer>, base: Option<String>) -> Result<String, JsValue> {
	Ok(self::pointer(&pointer)?.to_link(base.as_deref().unwrap_or(DEFAULT_LINK_BASE)))
}

/// Whether two pointers name the same stack.
#[wasm_bindgen(js_name = sameStack)]
pub fn same_stack(pointer: Ts<Pointer>, other: Ts<Pointer>) -> Result<bool, JsValue> {
	Ok(self::pointer(&pointer)?.same_stack(&self::pointer(&other)?))
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
pub fn stack_id(pointer: Ts<Pointer>) -> Result<String, JsValue> {
	Ok(self::pointer(&pointer)?.stack_id())
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
	let spec = spec.to_rust().map_err(|_| type_error("a spec needs url and sha256 strings"))?;
	spec.check(bytes).map_err(throw)
}

/// The 0x-prefixed genesis hash a light spec implies.
#[wasm_bindgen(js_name = lightSpecGenesis)]
pub fn light_spec_genesis(bytes: &[u8]) -> Result<String, JsValue> {
	Ok(format::to_hex(&crate::light_spec_genesis(bytes).map_err(throw)?))
}
