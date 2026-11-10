//! Kettle's log transfer over iroh, when the worker asks for it.
//!
//! In a browser an iroh endpoint reaches peers only through a relay, over a
//! WebSocket: no direct UDP. Each node binds its own endpoint, with no address
//! lookup, on either n0's public test relays (the relay map iroh 1.4.1 ships as
//! its default, `iroh::defaults::prod`) or one custom relay, and publishes the
//! endpoint's address, which names its home relay, in its outboxes. A node
//! whose endpoint reaches no relay in time runs without a log transfer, on the
//! Statement Store alone, and says why in its [`IrohStatus`].
//!
//! A custom relay URL is published in every outbox, so it carries nothing but
//! a scheme, a host and a port: [`relay_url`] refuses anything else. iroh
//! dials whatever relay a peer's address names, so the endpoint connects only
//! to peers whose address names relays of its own map ([`AllowedRelays`]).

use core::time::Duration;

use iroh::{
	EndpointAddr, RelayMap, RelayUrl, TransportAddr,
	endpoint::{BeforeConnectOutcome, EndpointHooks},
};
use js_sys::{Array, Object, Reflect};
use kettle_chat::TraceEvent;
use wasm_bindgen::JsValue;

use crate::ports::{hex_of, set};

/// How long a node waits for its endpoint to reach a relay, unless told.
const ONLINE_TIMEOUT: Duration = Duration::from_secs(10);

/// Why a relay URL is refused. It does not repeat the value, which may hold
/// a secret.
pub const BAD_RELAY_URL: &str = "the relay must be an http or https URL with a host, and no user, password, path, query or fragment";

/// Why `options.iroh` is refused.
const BAD_IROH: &str =
	"options.iroh must be { relays: \"public\" } or { relays: \"custom\", relayUrl }";

/// The relays a node's endpoint may use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Relays {
	/// n0's public test relays, as iroh 1.4.1 ships them.
	Public,
	/// One relay the user named.
	Custom(RelayUrl),
}

impl Relays {
	/// What `options.iroh` asks for: None without it, an error text, which
	/// repeats nothing of the value, for anything else.
	pub fn from_options(options: &JsValue) -> Result<Option<Self>, &'static str> {
		let config = Reflect::get(options, &"iroh".into()).unwrap_or(JsValue::UNDEFINED);
		if config.is_undefined() || config.is_null() {
			return Ok(None);
		}
		let field = |name: &str| {
			Reflect::get(&config, &name.into()).ok().and_then(|value| value.as_string())
		};
		match field("relays").as_deref() {
			Some("public") => Ok(Some(Self::Public)),
			Some("custom") => field("relayUrl")
				.and_then(|text| relay_url(&text))
				.map(|url| Some(Self::Custom(url)))
				.ok_or(BAD_RELAY_URL),
			_ => Err(BAD_IROH),
		}
	}

	fn name(&self) -> &'static str {
		match self {
			Self::Public => "public",
			Self::Custom(_) => "custom",
		}
	}

	pub fn map(&self) -> RelayMap {
		match self {
			Self::Public => iroh::defaults::prod::default_relay_map(),
			Self::Custom(url) => RelayMap::from(url.clone()),
		}
	}

	/// The relays of [`Self::map`], sorted.
	pub fn urls(&self) -> Vec<RelayUrl> {
		let mut urls: Vec<RelayUrl> = self.map().urls();
		urls.sort();
		urls
	}
}

/// What a node runs of the log transfer.
pub enum IrohStatus {
	/// No relays were asked for: no endpoint.
	Off,
	/// The endpoint reached `relay`, its home relay, which its address names.
	On { relays: &'static str, relay: String, endpoint: String },
	/// Relays were asked for, but the endpoint reached none, or the request
	/// did not read (`relays` None). `relay` names a custom relay only.
	Unavailable { relays: Option<&'static str>, relay: Option<String>, reason: String },
}

/// The relay `text` names: an http(s) URL of a host and an optional port only.
pub fn relay_url(text: &str) -> Option<RelayUrl> {
	let url: RelayUrl = text.parse().ok()?;
	relay_key(&url).is_some().then_some(url)
}

/// What names a relay: its scheme, host without a final dot, and port. None
/// for a URL with a user, password, path, query or fragment.
fn relay_key(url: &RelayUrl) -> Option<(String, String, u16)> {
	let plain = matches!(url.scheme(), "http" | "https") &&
		url.username().is_empty() &&
		url.password().is_none() &&
		url.path() == "/" &&
		url.query().is_none() &&
		url.fragment().is_none();
	let host = url.host_str()?.trim_end_matches('.');
	let port = url.port_or_known_default()?;
	(plain && !host.is_empty()).then(|| (url.scheme().to_string(), host.to_string(), port))
}

/// Lets the endpoint connect only to a peer whose address names at least one
/// relay, each of them one of `relays`, and no custom transport. iroh 1.4.1
/// learns a remote's relay only from the address passed to `connect` and from
/// address lookups, of which this endpoint has none.
#[derive(Debug)]
struct AllowedRelays {
	relays: Vec<(String, String, u16)>,
}

impl AllowedRelays {
	fn new(relays: &Relays) -> Self {
		Self { relays: relays.urls().iter().filter_map(relay_key).collect() }
	}

	fn allows(&self, remote: &EndpointAddr) -> bool {
		let allowed = |url: &RelayUrl| relay_key(url).is_some_and(|key| self.relays.contains(&key));
		remote.relay_urls().next().is_some() &&
			remote.addrs.iter().all(|addr| match addr {
				TransportAddr::Relay(url) => allowed(url),
				// A browser has no IP transport: a native peer's IP
				// addresses are never dialed from here.
				TransportAddr::Ip(_) => true,
				_ => false,
			})
	}
}

impl EndpointHooks for AllowedRelays {
	async fn before_connect<'a>(
		&'a self,
		remote: &'a EndpointAddr,
		_alpn: &'a [u8],
	) -> BeforeConnectOutcome {
		if self.allows(remote) {
			BeforeConnectOutcome::Accept
		} else {
			BeforeConnectOutcome::Reject
		}
	}
}

impl IrohStatus {
	pub fn to_js(&self) -> JsValue {
		let object = Object::new();
		match self {
			Self::Off => set(&object, "state", "off"),
			Self::On { relays, relay, endpoint } => {
				set(&object, "state", "on");
				set(&object, "relays", *relays);
				set(&object, "relay", relay.as_str());
				set(&object, "endpoint", endpoint.as_str());
			},
			Self::Unavailable { relays, relay, reason } => {
				set(&object, "state", "unavailable");
				if let Some(relays) = relays {
					set(&object, "relays", *relays);
				}
				if let Some(relay) = relay {
					set(&object, "relay", relay.as_str());
				}
				set(&object, "reason", reason.as_str());
			},
		}
		object.into()
	}
}

/// The endpoint `options.iroh` asks for (`{ relays: "public" }` or
/// `{ relays: "custom", relayUrl }`, with `onlineTimeoutMs?`), once its
/// address names a relay of its map; none without `options.iroh`.
pub async fn bind(options: &JsValue) -> (Option<iroh::Endpoint>, IrohStatus) {
	let relays = match Relays::from_options(options) {
		Ok(None) => return (None, IrohStatus::Off),
		Ok(Some(relays)) => relays,
		Err(reason) => {
			return (
				None,
				IrohStatus::Unavailable { relays: None, relay: None, reason: reason.into() },
			);
		},
	};
	let custom = match &relays {
		Relays::Custom(url) => Some(url.to_string()),
		Relays::Public => None,
	};
	let unavailable = |reason: String| {
		(
			None,
			IrohStatus::Unavailable { relays: Some(relays.name()), relay: custom.clone(), reason },
		)
	};
	let timeout = Reflect::get(options, &"iroh".into())
		.and_then(|config| Reflect::get(&config, &"onlineTimeoutMs".into()))
		.ok()
		.and_then(|value| value.as_f64())
		.map_or(ONLINE_TIMEOUT, |ms| Duration::from_millis(ms as u64));
	let endpoint = match iroh::Endpoint::builder(iroh::endpoint::presets::Minimal)
		.relay_mode(iroh::RelayMode::Custom(relays.map()))
		.hooks(AllowedRelays::new(&relays))
		.bind()
		.await
	{
		Ok(endpoint) => endpoint,
		Err(error) => return unavailable(format!("the endpoint did not bind: {error}")),
	};
	let online = n0_future::time::timeout(timeout, endpoint.online()).await.is_ok();
	let urls = relays.urls();
	let home = endpoint.addr().relay_urls().find(|url| urls.contains(url)).cloned();
	let (true, Some(home)) = (online, home) else {
		endpoint.close().await;
		return unavailable(format!("the endpoint reached no relay in {} ms", timeout.as_millis()));
	};
	let status = IrohStatus::On {
		relays: relays.name(),
		relay: home.to_string(),
		endpoint: endpoint.id().to_string(),
	};
	(Some(endpoint), status)
}

/// Whether a node started with `options` would dial a peer whose address
/// names `relays`: the decision of its endpoint's [`AllowedRelays`] hook.
pub fn allows(options: &JsValue, relays: &[String]) -> Result<bool, &'static str> {
	let Some(own) = Relays::from_options(options)? else { return Ok(false) };
	let remote = EndpointAddr::from_parts(
		iroh::SecretKey::from_bytes(&[1; 32]).public(),
		relays.iter().filter_map(|text| text.parse().ok()).map(TransportAddr::Relay),
	);
	Ok(relays.iter().all(|text| text.parse::<RelayUrl>().is_ok()) &&
		AllowedRelays::new(&own).allows(&remote))
}

/// The relays a node started with `options` may use, or the reason it refuses them.
pub fn relays_for(options: &JsValue) -> Result<Array, &'static str> {
	Ok(match Relays::from_options(options)? {
		None => Array::new(),
		Some(relays) => relays.urls().iter().map(|url| JsValue::from(url.to_string())).collect(),
	})
}

fn range(range: &Option<(u64, u64)>) -> JsValue {
	match range {
		Some((first, last)) => Array::of2(&(*first as f64).into(), &(*last as f64).into()).into(),
		None => JsValue::NULL,
	}
}

/// The trace events of the log transfer, and the outboxes that tell a node
/// what it misses, as plain objects; null for the others.
pub fn trace_to_js(event: &TraceEvent) -> JsValue {
	let object = Object::new();
	match event {
		TraceEvent::OutboxReceived { signer, envelopes, head_seq, bytes, .. } => {
			set(&object, "type", "outboxReceived");
			set(&object, "signer", hex_of(signer));
			set(&object, "envelopes", range(envelopes));
			set(&object, "headSeq", *head_seq as f64);
			set(&object, "bytes", *bytes as f64);
		},
		TraceEvent::FetchStarted { sender, from, .. } => {
			set(&object, "type", "fetchStarted");
			set(&object, "sender", hex_of(sender));
			set(&object, "from", *from as f64);
		},
		TraceEvent::FetchFinished { sender, result, .. } => {
			set(&object, "type", "fetchFinished");
			set(&object, "sender", hex_of(sender));
			match result {
				Ok(count) => set(&object, "count", *count as f64),
				Err(error) => set(&object, "error", error.as_str()),
			}
		},
		_ => return JsValue::NULL,
	}
	object.into()
}
