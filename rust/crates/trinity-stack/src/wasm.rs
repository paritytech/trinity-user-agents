//! The wasm-bindgen surface `@parity/trinity-stack` wraps. Values cross as plain JS objects of the
//! crate's own types, and errors as a `TrinityStackError` carrying the rule's [`Error::code`].

use crate::{
	Accepted, Block, DEFAULT_LINK_BASE, Descriptor, Document, DotnsAnswer, DotnsLookedUp,
	DotnsRead, DotnsRecord, DotnsStep, Error, FetchRequest, Fetched, Pointer, PublishState,
	Refresh, Refreshed, Rendered, Served, SpecFile, StackRead, Verified, accept as accept_fetched,
	format, open as open_bytes,
};
use serde::Serialize;
use tsify::{Ts, Tsify};
use wasm_bindgen::prelude::*;

/// A rule error as a JS `Error` named `TrinityStackError`, carrying the rule's code.
fn to_js_error(error: Error) -> JsValue {
	let converted = js_sys::Error::new(&error.to_string());
	converted.set_name("TrinityStackError");
	js_sys::Reflect::set(&converted, &"code".into(), &error.code().into())
		.expect("setting a property on a fresh Error cannot fail");
	converted.into()
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
	Pointer::new(&url, string_field(&value, "key")?.as_deref()).map_err(to_js_error)
}

fn secret_key(secret: &[u8]) -> Result<[u8; 32], JsValue> {
	secret.try_into().map_err(|_| to_js_error(Error::SecretKeyLength(secret.len())))
}

/// What `parse` reads. No key was checked, so unlike [`Verified`] it says only whether the
/// descriptor came in an envelope.
#[derive(Serialize, Tsify)]
#[tsify(hashmap_as_object)]
pub struct ParsedDocument {
	/// The descriptor.
	pub descriptor: Descriptor,
	/// Whether the document is a signed envelope, whose signature nothing has checked.
	pub envelope: bool,
}

/// Parses a document without checking a key.
#[wasm_bindgen]
pub fn parse(bytes: &[u8]) -> Result<Ts<ParsedDocument>, JsValue> {
	let parsed = match Document::parse(bytes).map_err(to_js_error)? {
		Document::Plain(descriptor) => ParsedDocument { descriptor, envelope: false },
		Document::Envelope { descriptor, .. } => ParsedDocument { descriptor, envelope: true },
	};
	to_js(&parsed)
}

/// Opens a document under a pointer's rules.
#[wasm_bindgen]
pub fn open(bytes: &[u8], pointer: Ts<Pointer>) -> Result<Ts<Verified>, JsValue> {
	to_js(&open_bytes(bytes, &self::pointer(&pointer)?).map_err(to_js_error)?)
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
			.map_err(to_js_error)?,
	)
}

/// The pointer a link carries.
#[wasm_bindgen(js_name = decodeLink)]
pub fn decode_link(link: &str) -> Result<Ts<Pointer>, JsValue> {
	to_js(&Pointer::from_link(link).map_err(to_js_error)?)
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
	Ok(format::to_hex(&crate::content_hash(descriptor).map_err(to_js_error)?))
}

/// Renders a model as a checked descriptor, keeping `sequence` while the content matches
/// `previous`.
#[wasm_bindgen]
pub fn render(
	model: &[u8],
	generation: &str,
	previous: Option<Ts<PublishState>>,
	now_unix_seconds: f64,
) -> Result<Ts<Rendered>, JsValue> {
	let previous = previous
		.map(|state| state.to_rust())
		.transpose()
		.map_err(|_| type_error("a publish state needs sequence and contentHash"))?;
	let rendered = crate::render(model, generation, previous.as_ref(), now_unix_seconds as u64)
		.map_err(to_js_error)?;
	to_js(&rendered)
}

/// A plain descriptor in its canonical form: sorted keys, unknown members kept.
#[wasm_bindgen]
pub fn canonical(descriptor: &[u8]) -> Result<Vec<u8>, JsValue> {
	crate::canonical(descriptor).map_err(to_js_error)
}

/// What identifies a pointer's stack; readers key what they store by it.
#[wasm_bindgen(js_name = stackId)]
pub fn stack_id(pointer: Ts<Pointer>) -> Result<String, JsValue> {
	Ok(self::pointer(&pointer)?.stack_id())
}

/// Wraps a plain descriptor in a signed envelope.
#[wasm_bindgen]
pub fn sign(descriptor: &[u8], secret: &[u8]) -> Result<Vec<u8>, JsValue> {
	crate::sign(descriptor, &secret_key(secret)?).map_err(to_js_error)
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
	spec.check(bytes).map_err(to_js_error)
}

/// The 0x-prefixed genesis hash a light spec implies.
#[wasm_bindgen(js_name = lightSpecGenesis)]
pub fn light_spec_genesis(bytes: &[u8]) -> Result<String, JsValue> {
	Ok(format::to_hex(&crate::light_spec_genesis(bytes).map_err(to_js_error)?))
}

fn hex_value(value: &str) -> Result<Vec<u8>, JsValue> {
	value
		.strip_prefix("0x")
		.and_then(|digits| hex::decode(digits).ok())
		.ok_or_else(|| type_error(&format!("{value:?} is not 0x-prefixed hex")))
}

/// Reads a dotNS record as `encodeDotnsRecord` wrote it, checking every address.
#[wasm_bindgen(js_name = decodeDotnsRecord)]
pub fn decode_dotns_record(bytes: &[u8]) -> Result<Ts<DotnsRecord>, JsValue> {
	to_js(&DotnsRecord::parse(bytes).map_err(to_js_error)?)
}

/// A dotNS record as a project commits it.
#[wasm_bindgen(js_name = encodeDotnsRecord)]
pub fn encode_dotns_record(record: Ts<DotnsRecord>) -> Result<Vec<u8>, JsValue> {
	let record = self::record(Some(record))?.expect("a record was given");
	record.check().map_err(to_js_error)?;
	Ok(record.to_json())
}

fn record(record: Option<Ts<DotnsRecord>>) -> Result<Option<DotnsRecord>, JsValue> {
	record
		.map(|record| record.to_rust())
		.transpose()
		.map_err(|_| type_error("a dotNS record needs generation, at and dotns"))
}

/// [`crate::DotnsLookup`] for JS.
#[wasm_bindgen(js_name = DotnsLookup)]
pub struct JsDotnsLookup(crate::DotnsLookup);

#[wasm_bindgen(js_class = DotnsLookup)]
impl JsDotnsLookup {
	/// A lookup for a descriptor of `generation`, read at Asset Hub block `at`.
	#[wasm_bindgen(constructor)]
	pub fn new(generation: String, at: Ts<Block>) -> Result<JsDotnsLookup, JsValue> {
		let at = at.to_rust().map_err(|_| type_error("a block needs number and hash"))?;
		Ok(Self(crate::DotnsLookup::new(generation, at).map_err(to_js_error)?))
	}

	/// What the lookup needs next.
	pub fn next(&self) -> Result<Ts<DotnsStep>, JsValue> {
		to_js(&self.0.next())
	}

	/// Takes the hex value at the storage key, undefined when it is unset.
	pub fn storage(&mut self, value: Option<String>) -> Result<(), JsValue> {
		let value = value.as_deref().map(hex_value).transpose()?;
		self.0.storage(value.as_deref()).map_err(to_js_error)
	}

	/// Takes the calls' answers in order: the hex data a call returned, or null when it
	/// reverted.
	pub fn answer(
		&mut self,
		#[wasm_bindgen(unchecked_param_type = "(string | null)[]")] answers: Vec<JsValue>,
	) -> Result<(), JsValue> {
		let answers = answers
			.iter()
			.map(|answer| match answer.as_string() {
				Some(data) => Ok(DotnsAnswer::Returned(hex_value(&data)?)),
				None if answer.is_null() => Ok(DotnsAnswer::Reverted),
				None => Err(type_error("an answer is hex data, or null for a revert")),
			})
			.collect::<Result<Vec<_>, _>>()?;
		self.0.answer(&answers).map_err(to_js_error)
	}
}

/// [`crate::StackReader`] for JS.
#[wasm_bindgen(js_name = StackReader)]
pub struct JsStackReader(crate::StackReader);

#[wasm_bindgen(js_class = StackReader)]
impl JsStackReader {
	/// A reader that refreshes by `refresh`: `"never"`, `"once"` or `"maxAge"`, the last
	/// with `maxAgeSeconds`.
	#[wasm_bindgen(constructor)]
	pub fn new(refresh: &str, max_age_seconds: Option<f64>) -> Result<JsStackReader, JsValue> {
		let refresh = match (refresh, max_age_seconds) {
			("never", None) => Refresh::Never,
			("once", None) => Refresh::Once,
			("maxAge", Some(seconds)) if seconds.is_finite() && seconds >= 1.0 => {
				Refresh::MaxAge(seconds as u64)
			},
			_ => {
				return Err(type_error(
					"refresh is never, once, or maxAge with a finite maxAgeSeconds of at least 1",
				));
			},
		};
		Ok(Self(crate::StackReader::new(refresh)))
	}

	/// Starts a stack from its build copy and the record the caller stored.
	pub fn load(
		&mut self,
		pointer: Ts<Pointer>,
		build: Option<Vec<u8>>,
		build_dotns: Option<Ts<DotnsRecord>>,
		stored: Option<Vec<u8>>,
	) -> Result<(), JsValue> {
		self.0
			.load(
				&self::pointer(&pointer)?,
				build.as_deref(),
				record(build_dotns)?,
				stored.as_deref(),
			)
			.map_err(to_js_error)
	}

	/// The copy to use now, and a fetch to make.
	pub fn get(
		&mut self,
		pointer: Ts<Pointer>,
		now_unix_seconds: f64,
	) -> Result<Ts<StackRead>, JsValue> {
		to_js(&self.0.get(&self::pointer(&pointer)?, now_unix_seconds as i64))
	}

	/// A fetch whatever the policy, unless one is out already.
	pub fn refresh(
		&mut self,
		pointer: Ts<Pointer>,
		now_unix_seconds: f64,
	) -> Result<Option<Ts<FetchRequest>>, JsValue> {
		self.0
			.refresh(&self::pointer(&pointer)?, now_unix_seconds as i64)
			.map(|request| to_js(&request))
			.transpose()
	}

	/// The `stackId`s whose background refresh is due.
	pub fn due(&self, now_unix_seconds: f64) -> Vec<String> {
		self.0.due(now_unix_seconds as i64)
	}

	/// Takes the answer to a fetch: the bytes and `ETag` served, `notModified` for a `304`, or
	/// no bytes when it failed.
	pub fn fetched(
		&mut self,
		pointer: Ts<Pointer>,
		document: Option<Vec<u8>>,
		etag: Option<String>,
		not_modified: bool,
		now_unix_seconds: f64,
	) -> Result<Ts<Refreshed>, JsValue> {
		let served = match (&document, not_modified) {
			(_, true) => Served::NotModified,
			(Some(bytes), false) => Served::Document { bytes, etag: etag.as_deref() },
			(None, false) => Served::Unreachable,
		};
		let refreshed = self
			.0
			.fetched(&self::pointer(&pointer)?, served, now_unix_seconds as i64)
			.map_err(to_js_error)?;
		to_js(&refreshed)
	}

	/// The dotNS record to use and a lookup to run; undefined until a descriptor is held.
	pub fn dotns(
		&mut self,
		pointer: Ts<Pointer>,
		now_unix_seconds: f64,
	) -> Result<Option<Ts<DotnsRead>>, JsValue> {
		self.0
			.dotns(&self::pointer(&pointer)?, now_unix_seconds as i64)
			.map(|read| to_js(&read))
			.transpose()
	}

	/// Takes a lookup's record, undefined when it failed.
	#[wasm_bindgen(js_name = lookedUp)]
	pub fn looked_up(
		&mut self,
		pointer: Ts<Pointer>,
		record: Option<Ts<DotnsRecord>>,
		now_unix_seconds: f64,
	) -> Result<Ts<DotnsLookedUp>, JsValue> {
		let looked_up = self
			.0
			.looked_up(&self::pointer(&pointer)?, self::record(record)?, now_unix_seconds as i64)
			.map_err(to_js_error)?;
		to_js(&looked_up)
	}
}
