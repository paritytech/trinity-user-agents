//! The wasm-bindgen surface `@parity/trinity-stack` wraps. Descriptors cross as JSON text, and
//! errors as a `TrinityStackError` carrying the rule's [`Error::code`].

use crate::{
	Document, Error, Fetched, Pointer, SpecFile, Status, accept as accept_fetched, format,
	open as open_bytes,
};
use wasm_bindgen::prelude::*;

fn throw(error: Error) -> JsValue {
	let thrown = js_sys::Error::new(&error.to_string());
	thrown.set_name("TrinityStackError");
	js_sys::Reflect::set(&thrown, &"code".into(), &error.code().into())
		.expect("setting a property on a fresh Error cannot fail");
	thrown.into()
}

fn secret_key(secret: &[u8]) -> Result<[u8; 32], JsValue> {
	secret
		.try_into()
		.map_err(|_| throw(Error::InvalidHex { field: "secret key", bytes: 32 }))
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
pub fn open(bytes: &[u8], url: &str, key: Option<String>) -> Result<String, JsValue> {
	let pointer = Pointer::new(url, key.as_deref()).map_err(throw)?;
	let stack = open_bytes(bytes, &pointer).map_err(throw)?;
	Ok(json(serde_json::json!({ "descriptor": stack.descriptor, "signed": stack.signed })))
}

/// What `accept` decided.
#[wasm_bindgen]
pub struct Accepted {
	/// One of `updated`, `unchanged`, `conflict`, `rollback`, `unreachable`, `refused`.
	#[wasm_bindgen(getter_with_clone)]
	pub status: String,
	/// The rule the fetched bytes broke, when `status` is `refused`.
	#[wasm_bindgen(getter_with_clone)]
	pub refused: Option<String>,
	/// The bytes to store.
	#[wasm_bindgen(getter_with_clone)]
	pub document: Vec<u8>,
	/// The descriptor to use, as JSON.
	#[wasm_bindgen(getter_with_clone)]
	pub descriptor: String,
	/// Whether a signature was checked.
	pub signed: bool,
	/// The chains restarted.
	#[wasm_bindgen(js_name = newGeneration)]
	pub new_generation: bool,
	/// Past `validUntil`.
	pub stale: bool,
	/// Past `expiresAt`.
	pub expired: bool,
}

/// Decides what a reader does with fetched bytes; `fetched` is undefined when the fetch failed.
#[wasm_bindgen]
pub fn accept(
	url: &str,
	key: Option<String>,
	stored: Option<Vec<u8>>,
	fetched: Option<Vec<u8>>,
	now_unix_seconds: f64,
) -> Result<Accepted, JsValue> {
	let pointer = Pointer::new(url, key.as_deref()).map_err(throw)?;
	let fetched = fetched.as_deref().map_or(Fetched::Unreachable, Fetched::Document);
	let accepted = accept_fetched(&pointer, stored.as_deref(), fetched, now_unix_seconds as i64)
		.map_err(throw)?;
	let refused = match &accepted.status {
		Status::Refused(error) => Some(error.code().to_owned()),
		_ => None,
	};
	Ok(Accepted {
		status: accepted.status.code().to_owned(),
		refused,
		document: accepted.document,
		descriptor: json(serde_json::json!(accepted.stack.descriptor)),
		signed: accepted.stack.signed,
		new_generation: accepted.new_generation,
		stale: accepted.stale,
		expired: accepted.expired,
	})
}

/// The pointer a link carries, as JSON `{ url, key? }`.
#[wasm_bindgen(js_name = decodeLink)]
pub fn decode_link(link: &str) -> Result<String, JsValue> {
	let pointer = Pointer::from_link(link).map_err(throw)?;
	let mut decoded = serde_json::json!({ "url": pointer.url().as_str() });
	if let Some(key) = pointer.key() {
		decoded["key"] = format::to_hex(key).into();
	}
	Ok(json(decoded))
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
pub fn check_spec(url: &str, sha256: &str, bytes: &[u8]) -> Result<(), JsValue> {
	SpecFile { url: url.to_owned(), sha256: sha256.to_owned() }
		.check(bytes)
		.map_err(throw)
}

/// The 0x-prefixed genesis hash a light spec implies.
#[wasm_bindgen(js_name = lightSpecGenesis)]
pub fn light_spec_genesis(bytes: &[u8]) -> Result<String, JsValue> {
	Ok(format::to_hex(&crate::light_spec_genesis(bytes).map_err(throw)?))
}

/// A QR code as an SVG document.
#[wasm_bindgen(js_name = qrSvg)]
pub fn qr_svg(text: &str) -> Result<String, JsValue> {
	crate::qr_svg(text).map_err(throw)
}

/// A QR code drawn for a terminal.
#[wasm_bindgen(js_name = qrTerminal)]
pub fn qr_terminal(text: &str) -> Result<String, JsValue> {
	crate::qr_terminal(text).map_err(throw)
}
