//! The ports Kettle's chat engine needs, served by JavaScript: a statement
//! store whose account is one conversation account, and a storage for the
//! engine's sealed state.
//!
//! The JavaScript object passed to [`crate::KettleNode::start`] holds these
//! functions:
//!
//! - `submit(statement)`: sign the statement as the account and submit it. Answers `{ status,
//!   reason? }`, `status` being `new`, `known`, `knownExpired`, `rejected` or `invalid`.
//! - `subscribe(filter, deliver, ended)`: call `deliver(statement)` with each signed statement the
//!   filter matches, the stored ones first, and `ended(reason)` when the subscription ends. Answers
//!   a function that stops it. The engine checks each statement's proof itself.
//! - `entropy(context)`: 32 bytes for this account and `context`.
//! - `requestAllowance()`: answers `allocated`, `rejected` or `notAvailable`.
//! - `read(name)`, `write(name, bytes)`, `remove(name)`: the storage.
//!
//! A statement crosses as the TrUAPI `Statement` object: `expiry` a bigint,
//! `channel`, `topics` and `data` `0x` hex, and `proof` the tagged
//! `Sr25519` or `Ed25519` proof.

use core::{
	fmt,
	pin::Pin,
	task::{Context, Poll},
};

use anyhow::{Context as _, anyhow, bail};
use futures::{Stream, StreamExt, channel::mpsc};
use js_sys::{Array, BigInt, Function, Object, Promise, Reflect, Uint8Array};
use kettle_proto::{
	statement::{AccountId, MAX_TOPICS, Proof, Statement},
	storage::{Storage, StorageError},
	store::{AllowanceOutcome, Rejection, StatementStore, SubmitResult, TopicFilter},
};
use n0_future::boxed::{BoxFuture, BoxStream};
use send_wrapper::SendWrapper;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;

/// The JavaScript ports of one conversation account.
#[derive(Clone)]
pub struct JsPorts {
	account: AccountId,
	ports: SendWrapper<JsValue>,
}

impl fmt::Debug for JsPorts {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(formatter, "JsPorts({})", hex::encode(self.account))
	}
}

impl JsPorts {
	pub fn new(account: AccountId, ports: JsValue) -> Self {
		Self { account, ports: SendWrapper::new(ports) }
	}

	fn function(&self, name: &str) -> anyhow::Result<Function> {
		Reflect::get(&self.ports, &JsValue::from_str(name))
			.ok()
			.and_then(|value| value.dyn_into::<Function>().ok())
			.with_context(|| format!("the ports have no function `{name}`"))
	}

	/// Call port `name` and wait for its answer when it is a promise.
	fn call(&self, name: &'static str, args: Vec<JsValue>) -> BoxFuture<anyhow::Result<JsValue>> {
		let this = self.clone();
		Box::pin(async move {
			let function = this.function(name)?;
			let answer = function
				.apply(&this.ports, &args.into_iter().collect::<Array>())
				.map_err(|error| anyhow!("{name}: {}", error_text(&error)))?;
			if let Some(promise) = answer.dyn_ref::<Promise>() {
				JsFuture::from(promise.clone())
					.await
					.map_err(|error| anyhow!("{name}: {}", error_text(&error)))
			} else {
				Ok(answer)
			}
		})
	}
}

impl StatementStore for JsPorts {
	fn account(&self) -> AccountId {
		self.account
	}

	fn entropy(&self, context: &[u8]) -> BoxFuture<anyhow::Result<[u8; 32]>> {
		let call = self.call("entropy", vec![Uint8Array::from(context).into()]);
		Box::pin(async move { fixed::<32>(&bytes_of(&call.await?)?, "entropy") })
	}

	fn submit(&self, statement: Statement) -> BoxFuture<anyhow::Result<SubmitResult>> {
		let call = self.call("submit", vec![statement_to_js(&statement)]);
		Box::pin(async move { submit_result(&call.await?) })
	}

	fn subscribe(&self, filter: TopicFilter) -> BoxFuture<anyhow::Result<BoxStream<Statement>>> {
		let this = self.clone();
		Box::pin(async move {
			let subscribe = this.function("subscribe")?;
			let (sender, receiver) = mpsc::unbounded();
			let delivering = sender.clone();
			let deliver = Closure::<dyn FnMut(JsValue)>::new(move |value: JsValue| {
				// A statement that does not read is one the engine would drop anyway.
				if let Ok(statement) = statement_from_js(&value) {
					delivering.unbounded_send(statement).ok();
				}
			});
			let ended = Closure::<dyn FnMut(JsValue)>::new(move |_reason: JsValue| {
				sender.close_channel();
			});
			let stop = subscribe
				.call3(&this.ports, &filter_to_js(&filter), deliver.as_ref(), ended.as_ref())
				.map_err(|error| anyhow!("subscribe: {}", error_text(&error)))?
				.dyn_into::<Function>()
				.map_err(|_| anyhow!("subscribe answered no function to stop it"))?;
			let stream: BoxStream<Statement> = Box::pin(JsSubscription {
				receiver,
				stop: Some(stop),
				_deliver: deliver,
				_ended: ended,
			});
			Ok(stream)
		})
	}

	fn request_allowance(&self) -> BoxFuture<anyhow::Result<AllowanceOutcome>> {
		let call = self.call("requestAllowance", vec![]);
		Box::pin(async move {
			Ok(match call.await?.as_string().as_deref() {
				Some("allocated") => AllowanceOutcome::Allocated,
				Some("rejected") => AllowanceOutcome::Rejected,
				_ => AllowanceOutcome::NotAvailable,
			})
		})
	}
}

impl Storage for JsPorts {
	fn read(&self, name: &str) -> BoxFuture<Result<Option<Vec<u8>>, StorageError>> {
		let call = self.call("read", vec![JsValue::from_str(name)]);
		Box::pin(async move {
			let value = call.await.map_err(storage_error)?;
			if value.is_undefined() || value.is_null() {
				return Ok(None);
			}
			bytes_of(&value).map(Some).map_err(storage_error)
		})
	}

	fn write(&self, name: &str, value: Vec<u8>) -> BoxFuture<Result<(), StorageError>> {
		let call =
			self.call("write", vec![JsValue::from_str(name), Uint8Array::from(&value[..]).into()]);
		Box::pin(async move { call.await.map(drop).map_err(storage_error) })
	}

	fn remove(&self, name: &str) -> BoxFuture<Result<(), StorageError>> {
		let call = self.call("remove", vec![JsValue::from_str(name)]);
		Box::pin(async move { call.await.map(drop).map_err(storage_error) })
	}
}

/// The statements of one JavaScript subscription, until it ends or is dropped.
struct JsSubscription {
	receiver: mpsc::UnboundedReceiver<Statement>,
	stop: Option<Function>,
	_deliver: Closure<dyn FnMut(JsValue)>,
	_ended: Closure<dyn FnMut(JsValue)>,
}

impl Stream for JsSubscription {
	type Item = Statement;

	fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Statement>> {
		self.receiver.poll_next_unpin(context)
	}
}

impl Drop for JsSubscription {
	fn drop(&mut self) {
		if let Some(stop) = self.stop.take() {
			stop.call0(&JsValue::NULL).ok();
		}
	}
}

fn storage_error(error: anyhow::Error) -> StorageError {
	StorageError(format!("{error:#}"))
}

/// What a thrown JavaScript value says.
pub fn error_text(error: &JsValue) -> String {
	if let Some(error) = error.dyn_ref::<js_sys::Error>() {
		return String::from(error.message());
	}
	error.as_string().unwrap_or_else(|| format!("{error:?}"))
}

pub fn hex_of(bytes: &[u8]) -> String {
	format!("0x{}", hex::encode(bytes))
}

pub fn parse_hex(value: &str) -> anyhow::Result<Vec<u8>> {
	let digits = value.strip_prefix("0x").context("hex must start with 0x")?;
	Ok(hex::decode(digits)?)
}

pub fn fixed<const N: usize>(bytes: &[u8], what: &str) -> anyhow::Result<[u8; N]> {
	bytes
		.try_into()
		.map_err(|_| anyhow!("{what} is {} bytes, not {N}", bytes.len()))
}

fn bytes_of(value: &JsValue) -> anyhow::Result<Vec<u8>> {
	if let Some(array) = value.dyn_ref::<Uint8Array>() {
		return Ok(array.to_vec());
	}
	match value.as_string() {
		Some(text) => parse_hex(&text),
		None => bail!("expected bytes, got {value:?}"),
	}
}

fn get(object: &JsValue, key: &str) -> JsValue {
	Reflect::get(object, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

pub fn set(object: &Object, key: &str, value: impl Into<JsValue>) {
	Reflect::set(object, &JsValue::from_str(key), &value.into()).ok();
}

fn hex_field<const N: usize>(object: &JsValue, key: &str) -> anyhow::Result<Option<[u8; N]>> {
	match get(object, key).as_string() {
		Some(text) => Ok(Some(fixed::<N>(&parse_hex(&text)?, key)?)),
		None => Ok(None),
	}
}

fn statement_to_js(statement: &Statement) -> JsValue {
	let object = Object::new();
	set(&object, "expiry", BigInt::from(statement.expiry));
	if let Some(channel) = statement.channel {
		set(&object, "channel", hex_of(&channel));
	}
	let topics: Array = statement.topics.iter().map(|topic| JsValue::from(hex_of(topic))).collect();
	set(&object, "topics", topics);
	if let Some(data) = &statement.data {
		set(&object, "data", hex_of(data));
	}
	object.into()
}

fn statement_from_js(value: &JsValue) -> anyhow::Result<Statement> {
	let proof = get(value, "proof");
	let tag = get(&proof, "tag").as_string().context("a statement has no proof")?;
	let signed = get(&proof, "value");
	let signature = hex_field::<64>(&signed, "signature")?.context("a proof has no signature")?;
	let signer = hex_field::<32>(&signed, "signer")?.context("a proof has no signer")?;
	let proof = match tag.as_str() {
		"Sr25519" => Proof::Sr25519 { signature, signer },
		"Ed25519" => Proof::Ed25519 { signature, signer },
		other => bail!("a {other} proof is not read here"),
	};
	let expiry = get(value, "expiry");
	let expiry = if expiry.is_bigint() {
		u64::try_from(expiry).map_err(|_| anyhow!("the expiry is not a u64"))?
	} else {
		expiry.as_f64().context("a statement has no expiry")? as u64
	};
	let topics = Array::from(&get(value, "topics"));
	// Kettle's encoding asserts this bound: a statement past it would panic the module.
	anyhow::ensure!(
		topics.length() as usize <= MAX_TOPICS,
		"a statement has {} topics, more than {MAX_TOPICS}",
		topics.length()
	);
	let topics = topics
		.iter()
		.map(|topic| {
			fixed::<32>(&parse_hex(&topic.as_string().context("a topic is not hex")?)?, "topic")
		})
		.collect::<anyhow::Result<Vec<_>>>()?;
	let data = match get(value, "data").as_string() {
		Some(text) => Some(parse_hex(&text)?),
		None => None,
	};
	Ok(Statement {
		proof: Some(proof),
		decryption_key: None,
		expiry,
		channel: hex_field::<32>(value, "channel")?,
		topics,
		data,
	})
}

fn filter_to_js(filter: &TopicFilter) -> JsValue {
	let object = Object::new();
	let (kind, topics) = match filter {
		TopicFilter::Any => ("any", &[][..]),
		TopicFilter::MatchAll(topics) => ("matchAll", &topics[..]),
		TopicFilter::MatchAny(topics) => ("matchAny", &topics[..]),
	};
	set(&object, "kind", kind);
	let topics: Array = topics.iter().map(|topic| JsValue::from(hex_of(topic))).collect();
	set(&object, "topics", topics);
	object.into()
}

fn submit_result(answer: &JsValue) -> anyhow::Result<SubmitResult> {
	let reason = get(answer, "reason").as_string().unwrap_or_default();
	Ok(match get(answer, "status").as_string().as_deref() {
		Some("new") => SubmitResult::New,
		Some("known") => SubmitResult::Known,
		Some("knownExpired") => SubmitResult::KnownExpired,
		Some("invalid") => SubmitResult::Invalid(reason),
		Some("rejected") => SubmitResult::Rejected(match reason.as_str() {
			"dataTooLarge" => Rejection::DataTooLarge,
			"channelPriorityTooLow" => Rejection::ChannelPriorityTooLow,
			"accountFull" => Rejection::AccountFull,
			"storeFull" => Rejection::StoreFull,
			"noAllowance" => Rejection::NoAllowance,
			_ => Rejection::Other(reason),
		}),
		other => bail!("submit answered status {other:?}"),
	})
}
