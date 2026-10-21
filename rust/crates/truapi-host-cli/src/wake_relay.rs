//! `truapi-host wake-relay`: a reference push relay for wake pushes.
//!
//! A host registers a device with `POST /v1/registrations`: what identifies
//! the device to a push service, the key the core made, and the watched
//! statement-store topics. The relay follows those topics on the People
//! statement store. For each new statement on a watched topic it signs a wake
//! push (topic, statement hash, send time, MAC) and gives it to its sink. The
//! push never carries statement content.
//!
//! This relay holds no APNs or FCM credentials, so it serves tests only.
//! Production needs a relay with push credentials (open item O27). Its sinks:
//! - `file:<path>` appends one JSON line for each push.
//! - `simulator` gives the payload to the app on a simulator: it writes the payload in
//!   `tmp/truapi-e2e/wake-pushes` of the app and posts the Darwin notification
//!   `io.parity.truapi.e2e.wake-push`, which the E2E build of the iOS app reads
//!   (`TrUAPIE2EWakePushFeed`).
//! - `simctl` runs `xcrun simctl push`. An iOS 27 simulator refuses a payload with no visible
//!   content and does not call the app delegate, so this sink does not wake an app there.
//!
//! The relay is host infrastructure. A product never reaches it: the core
//! registers the topics through the host's relay client.

use std::{
	collections::{HashMap, HashSet, VecDeque},
	io::Write,
	net::SocketAddr,
	path::PathBuf,
	sync::{Arc, Mutex},
	time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use subxt_rpcs::{RpcClient, client::rpc_params};
use tokio::{
	io::{AsyncReadExt, AsyncWriteExt},
	net::{TcpListener, TcpStream},
	task::JoinHandle,
};
use truapi::{
	WAKE_PUSH_PAYLOAD_KEY, WakePush,
	host_logic::statement_store::{
		MAX_MATCH_ANY_TOPICS, SUBSCRIBE_STATEMENT_METHOD, UNSUBSCRIBE_STATEMENT_METHOD,
		decode_signed_statement, parse_new_statements_result,
	},
	statement_hash,
};

use crate::network::Network;

/// Largest request body the relay reads.
const MAX_BODY: usize = 256 * 1024;

/// Wait before a dropped statement subscription opens again.
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// Statement hashes each device remembers, so a statement pushes once.
const SEEN_STATEMENTS: usize = 4096;

/// Run a reference push relay for wake pushes, for tests.
#[derive(Args, Debug, Clone)]
pub struct WakeRelayArgs {
	/// Loopback port of the registration endpoint.
	#[arg(long, default_value_t = 9988)]
	port: u16,
	/// Network whose People statement store the relay follows.
	#[arg(long, value_enum, default_value = "paseo-next-v2")]
	network: Network,
	/// People chain WebSocket endpoint, in place of the one of `--network`.
	#[arg(long)]
	people_ws: Option<String>,
	/// Where wake pushes go: `file:<path>` appends one JSON line for each
	/// push; `simulator` and `simctl` deliver to a device registered with
	/// platform `simctl` (its token is the simulator udid), see the module
	/// documentation. Repeat it for more than one sink.
	#[arg(long = "sink", required = true)]
	sinks: Vec<Sink>,
	/// A statement that comes in this time after a subscription opens is the
	/// store's backlog: the relay remembers it and sends no push for it.
	#[arg(long, default_value_t = 4000)]
	backlog_ms: u64,
	/// After a push for a topic of a device, the relay sends no other push
	/// for that topic of that device in this time. The wake of the first push
	/// reads every statement.
	#[arg(long, default_value_t = 20_000)]
	quiet_ms: u64,
}

/// A destination of wake pushes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sink {
	/// Append JSON lines to a file.
	File(PathBuf),
	/// Give the payload to the E2E wake push feed of the app on a simulator.
	Simulator,
	/// Deliver with `xcrun simctl push`.
	Simctl,
}

impl std::str::FromStr for Sink {
	type Err = String;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		match value {
			"simctl" => Ok(Self::Simctl),
			"simulator" => Ok(Self::Simulator),
			_ => match value.strip_prefix("file:") {
				Some(path) if !path.is_empty() => Ok(Self::File(PathBuf::from(path))),
				_ => Err(format!("unknown sink {value}; use file:<path>, simulator or simctl")),
			},
		}
	}
}

/// What identifies a device to its push service.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Device {
	/// `apns`, `apns-sandbox`, `fcm` or `simctl`.
	pub platform: String,
	/// The push token, or the simulator udid for `simctl`.
	pub token: String,
	/// The app id: the bundle id on iOS, the package name on Android.
	pub app: String,
}

/// A registration request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registration {
	/// The device.
	pub device: Device,
	/// The key of the device, `0x` hex of 32 bytes.
	pub key: String,
	/// The watched topics, `0x` hex of 32 bytes each. Empty: remove.
	pub topics: Vec<String>,
}

/// A checked registration.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Watch {
	device: Device,
	key: [u8; 32],
	topics: Vec<[u8; 32]>,
}

impl TryFrom<Registration> for Watch {
	type Error = anyhow::Error;

	fn try_from(registration: Registration) -> Result<Self> {
		let Registration { device, key, topics } = registration;
		if device.token.is_empty() || device.app.is_empty() {
			bail!("the device needs a token and an app id");
		}
		let topics = topics.iter().map(|topic| hex32(topic)).collect::<Result<Vec<_>>>()?;
		Ok(Self { device, key: hex32(&key)?, topics })
	}
}

fn hex32(text: &str) -> Result<[u8; 32]> {
	let bytes = hex::decode(text.strip_prefix("0x").unwrap_or(text))
		.with_context(|| format!("{text} is not hex"))?;
	bytes.try_into().map_err(|_| anyhow::anyhow!("{text} is not 32 bytes"))
}

/// The relay: one statement follower for each registered device.
struct Relay {
	people_ws: String,
	sinks: Vec<Sink>,
	backlog: Duration,
	quiet: Duration,
	devices: Mutex<HashMap<Device, (Watch, JoinHandle<()>)>>,
}

/// Run the relay until the process ends.
pub async fn run(args: WakeRelayArgs) -> Result<()> {
	let people_ws = match args.people_ws {
		Some(url) => url,
		None => args.network.cli_config()?.people_ws.to_string(),
	};
	let relay = Arc::new(Relay {
		people_ws,
		sinks: args.sinks,
		backlog: Duration::from_millis(args.backlog_ms),
		quiet: Duration::from_millis(args.quiet_ms),
		devices: Mutex::default(),
	});
	let address = SocketAddr::from(([127, 0, 0, 1], args.port));
	let listener = TcpListener::bind(address).await.with_context(|| format!("bind {address}"))?;
	println!(
		"[relay] wake relay ready on http://{} (statement store {}; no APNs or FCM credentials)",
		listener.local_addr()?,
		relay.people_ws
	);
	loop {
		let (stream, _) = listener.accept().await?;
		let relay = relay.clone();
		tokio::spawn(async move {
			if let Err(error) = relay.serve(stream).await {
				println!("[relay] request failed: {error:#}");
			}
		});
	}
}

impl Relay {
	/// Answer one HTTP request.
	async fn serve(self: Arc<Self>, mut stream: TcpStream) -> Result<()> {
		let (method, path, body) = read_request(&mut stream).await?;
		let (status, answer) = match (method.as_str(), path.as_str()) {
			("POST", "/v1/registrations") => match serde_json::from_slice::<Registration>(&body)
				.context("not a registration")
				.and_then(Watch::try_from)
			{
				Ok(watch) => {
					let topics = watch.topics.len();
					self.register(watch);
					("200 OK", json!({ "topics": topics }))
				},
				Err(error) => ("400 Bad Request", json!({ "error": format!("{error:#}") })),
			},
			("GET", "/v1/registrations") => ("200 OK", self.listing()),
			("GET", "/health") => ("200 OK", json!({ "ok": true })),
			_ => ("404 Not Found", json!({ "error": "unknown path" })),
		};
		let body = answer.to_string();
		let response = format!(
			"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
			body.len()
		);
		stream.write_all(response.as_bytes()).await?;
		Ok(())
	}

	/// Replace the watch of a device and follow its topics.
	fn register(self: &Arc<Self>, watch: Watch) {
		let device = watch.device.clone();
		let mut devices = self.devices.lock().expect("relay devices mutex poisoned");
		if let Some((_, follower)) = devices.remove(&device) {
			follower.abort();
		}
		println!(
			"[relay] registered {}:{} ({}) with {} topics",
			device.platform,
			device.token,
			device.app,
			watch.topics.len()
		);
		if watch.topics.is_empty() {
			return;
		}
		let follower = tokio::spawn(self.clone().follow(watch.clone()));
		devices.insert(device, (watch, follower));
	}

	fn listing(&self) -> Value {
		let devices = self.devices.lock().expect("relay devices mutex poisoned");
		Value::Array(
			devices
				.values()
				.map(|(watch, _)| {
					json!({
						"device": watch.device,
						"topics": watch.topics.iter().map(|topic| format!("0x{}", hex::encode(topic))).collect::<Vec<_>>(),
					})
				})
				.collect(),
		)
	}

	/// Follow the topics of one device until its registration changes.
	async fn follow(self: Arc<Self>, watch: Watch) {
		let mut state = Follow::new(self.quiet);
		loop {
			if let Err(error) = self.follow_once(&watch, &mut state).await {
				println!("[relay] {}: statement subscription ended: {error:#}", watch.device.token);
			}
			tokio::time::sleep(RECONNECT_DELAY).await;
		}
	}

	async fn follow_once(&self, watch: &Watch, state: &mut Follow) -> Result<()> {
		let rpc = RpcClient::from_insecure_url(&self.people_ws)
			.await
			.with_context(|| format!("connect {}", self.people_ws))?;
		let mut subscriptions = Vec::new();
		for chunk in watch.topics.chunks(MAX_MATCH_ANY_TOPICS) {
			let topics: Vec<String> =
				chunk.iter().map(|topic| format!("0x{}", hex::encode(topic))).collect();
			let subscription = rpc
				.subscribe::<Value>(
					SUBSCRIBE_STATEMENT_METHOD,
					rpc_params![json!({ "matchAny": topics })],
					UNSUBSCRIBE_STATEMENT_METHOD,
				)
				.await
				.context("subscribe to statements")?;
			subscriptions.push(subscription);
		}
		let opened = Instant::now();
		let mut merged = futures::stream::select_all(subscriptions);
		use futures::StreamExt;
		while let Some(notification) = merged.next().await {
			let notification = notification.context("statement notification")?;
			let statements = parse_new_statements_result(String::new(), &notification)
				.map_err(|error| anyhow::anyhow!("{error}"))?
				.statements;
			let backlog = opened.elapsed() < self.backlog;
			for statement in statements {
				if let Some(push) = state.push_for(watch, &statement, backlog) {
					self.deliver(&watch.device, &push);
				}
			}
		}
		bail!("the subscription closed")
	}

	/// Give one push to each sink.
	fn deliver(&self, device: &Device, push: &WakePush) {
		let payload = json!({
			"aps": { "content-available": 1 },
			WAKE_PUSH_PAYLOAD_KEY: push.to_json(),
		});
		println!(
			"[relay] wake push for {}:{} topic 0x{} statement 0x{}",
			device.platform,
			device.token,
			hex::encode(push.topic),
			hex::encode(push.statement_hash)
		);
		for sink in &self.sinks {
			let delivered = match sink {
				Sink::File(path) => {
					append_line(path, &json!({ "device": device, "payload": payload }))
				},
				Sink::Simulator if device.platform == "simctl" => feed_simulator(device, &payload),
				Sink::Simctl if device.platform == "simctl" => simctl_push(device, &payload),
				Sink::Simulator | Sink::Simctl => continue,
			};
			if let Err(error) = delivered {
				println!("[relay] sink {sink:?} failed: {error:#}");
			}
		}
	}
}

/// What the relay remembers of the statements of one device.
struct Follow {
	seen: HashSet<[u8; 32]>,
	order: VecDeque<[u8; 32]>,
	last_push: HashMap<[u8; 32], Instant>,
	quiet: Duration,
}

impl Follow {
	fn new(quiet: Duration) -> Self {
		Self { seen: HashSet::new(), order: VecDeque::new(), last_push: HashMap::new(), quiet }
	}

	/// The push for `statement`, when it is new, on a watched topic, not
	/// backlog, and not in the quiet time of an earlier push for its topic.
	fn push_for(&mut self, watch: &Watch, statement: &[u8], backlog: bool) -> Option<WakePush> {
		let hash = statement_hash(statement);
		if !self.seen.insert(hash) {
			return None;
		}
		self.order.push_back(hash);
		if self.order.len() > SEEN_STATEMENTS &&
			let Some(oldest) = self.order.pop_front()
		{
			self.seen.remove(&oldest);
		}
		if backlog {
			return None;
		}
		let topics = decode_signed_statement(statement).ok()?.topics;
		let topic = topics.into_iter().find(|topic| watch.topics.contains(topic))?;
		let now = Instant::now();
		if self
			.last_push
			.get(&topic)
			.is_some_and(|last| now.duration_since(*last) < self.quiet)
		{
			return None;
		}
		self.last_push.insert(topic, now);
		Some(WakePush::sign(&watch.key, topic, hash, unix_millis()))
	}
}

fn unix_millis() -> u64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|elapsed| elapsed.as_millis() as u64)
		.unwrap_or_default()
}

fn append_line(path: &PathBuf, line: &Value) -> Result<()> {
	let mut file = std::fs::OpenOptions::new()
		.create(true)
		.append(true)
		.open(path)
		.with_context(|| format!("open {}", path.display()))?;
	writeln!(file, "{line}")?;
	Ok(())
}

/// Put `payload` in the wake push feed of the app on a simulator device and
/// tell the app.
fn feed_simulator(device: &Device, payload: &Value) -> Result<()> {
	let container = std::process::Command::new("xcrun")
		.args(["simctl", "get_app_container", &device.token, &device.app, "data"])
		.output()
		.context("run xcrun simctl get_app_container")?;
	if !container.status.success() {
		bail!("the app {} is not on {}", device.app, device.token);
	}
	let directory = PathBuf::from(String::from_utf8_lossy(&container.stdout).trim())
		.join("tmp/truapi-e2e/wake-pushes");
	std::fs::create_dir_all(&directory)?;
	let name = format!(
		"{}-{}.json",
		unix_millis(),
		&hex::encode(statement_hash(payload.to_string().as_bytes()))[..8]
	);
	let partial = directory.join(format!("{name}.partial"));
	std::fs::write(&partial, payload.to_string())?;
	std::fs::rename(&partial, directory.join(&name))?;
	let posted = std::process::Command::new("xcrun")
		.args([
			"simctl",
			"spawn",
			&device.token,
			"notifyutil",
			"-p",
			"io.parity.truapi.e2e.wake-push",
		])
		.output()
		.context("run xcrun simctl spawn notifyutil")?;
	if !posted.status.success() {
		bail!("notifyutil: {}", String::from_utf8_lossy(&posted.stderr).trim());
	}
	println!("[relay] wake push {name} given to {} on {}", device.app, device.token);
	Ok(())
}

/// Deliver `payload` to the app of a simulator device.
fn simctl_push(device: &Device, payload: &Value) -> Result<()> {
	let file = tempfile::Builder::new().suffix(".apns").tempfile()?;
	std::fs::write(file.path(), payload.to_string())?;
	let output = std::process::Command::new("xcrun")
		.args(["simctl", "push", &device.token, &device.app])
		.arg(file.path())
		.output()
		.context("run xcrun simctl push")?;
	if !output.status.success() {
		bail!("simctl push: {}", String::from_utf8_lossy(&output.stderr).trim());
	}
	println!("[relay] simctl push delivered to {} {}", device.token, device.app);
	Ok(())
}

/// Read the method, the path and the body of one HTTP/1.1 request.
async fn read_request(stream: &mut TcpStream) -> Result<(String, String, Vec<u8>)> {
	let mut buffer = Vec::new();
	let head_end = loop {
		let mut chunk = [0u8; 4096];
		let read = stream.read(&mut chunk).await?;
		if read == 0 {
			bail!("the connection closed before the request head");
		}
		buffer.extend_from_slice(&chunk[..read]);
		if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
			break end + 4;
		}
		if buffer.len() > MAX_BODY {
			bail!("the request head is too long");
		}
	};
	let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
	let mut lines = head.lines();
	let mut request_line = lines.next().unwrap_or_default().split_whitespace();
	let method = request_line.next().unwrap_or_default().to_string();
	let path = request_line.next().unwrap_or_default().to_string();
	let length = lines
		.filter_map(|line| line.split_once(':'))
		.find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
		.map(|(_, value)| value.trim().parse::<usize>())
		.transpose()
		.context("bad content-length")?
		.unwrap_or(0);
	if length > MAX_BODY {
		bail!("the request body is too long");
	}
	let mut body = buffer[head_end..].to_vec();
	while body.len() < length {
		let mut chunk = vec![0u8; length - body.len()];
		let read = stream.read(&mut chunk).await?;
		if read == 0 {
			bail!("the connection closed before the request body");
		}
		body.extend_from_slice(&chunk[..read]);
	}
	body.truncate(length);
	Ok((method, path, body))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn watch() -> Watch {
		Watch {
			device: Device {
				platform: "simctl".to_string(),
				token: "UDID".to_string(),
				app: "io.example".to_string(),
			},
			key: [4; 32],
			topics: vec![[1; 32]],
		}
	}

	fn statement(topic: [u8; 32], data: u8) -> Vec<u8> {
		truapi::host_logic::statement_store::signed_statement_to_scale(
			truapi::latest::SignedStatement {
				proof: truapi::latest::StatementProof::Sr25519 {
					signature: [0; 64],
					signer: [0; 32],
				},
				decryption_key: None,
				expiry: Some(1),
				channel: None,
				topics: vec![topic],
				data: Some(vec![data]),
			},
		)
		.expect("a statement encodes")
	}

	#[test]
	fn sinks_parse_from_the_command_line() {
		assert_eq!("simctl".parse(), Ok(Sink::Simctl));
		assert_eq!("simulator".parse(), Ok(Sink::Simulator));
		assert_eq!("file:/tmp/p.jsonl".parse(), Ok(Sink::File("/tmp/p.jsonl".into())));
		assert!("apns".parse::<Sink>().is_err());
	}

	// The relay pushes once for a new statement on a watched topic, signs it
	// with the key of the device, and holds back the backlog, repeats, other
	// topics and a second push in the quiet time.
	#[test]
	fn a_new_statement_on_a_watched_topic_pushes_once() {
		let watch = watch();
		let mut follow = Follow::new(Duration::from_secs(60));

		assert_eq!(follow.push_for(&watch, &statement([1; 32], 0), true), None, "backlog");
		let push = follow.push_for(&watch, &statement([1; 32], 1), false).expect("a push");
		assert_eq!(push.topic, [1; 32]);
		assert_eq!(push.statement_hash, statement_hash(&statement([1; 32], 1)));
		assert_eq!(push, WakePush::sign(&[4; 32], [1; 32], push.statement_hash, push.sent_at_ms));
		assert_eq!(follow.push_for(&watch, &statement([1; 32], 1), false), None, "repeat");
		assert_eq!(follow.push_for(&watch, &statement([2; 32], 2), false), None, "other topic");
		assert_eq!(follow.push_for(&watch, &statement([1; 32], 3), false), None, "quiet time");
	}

	#[test]
	fn a_registration_needs_hex_keys_and_topics() {
		let registration = |key: &str| Registration {
			device: watch().device,
			key: key.to_string(),
			topics: vec![format!("0x{}", "01".repeat(32))],
		};
		assert_eq!(
			Watch::try_from(registration(&format!("0x{}", "04".repeat(32)))).ok(),
			Some(watch())
		);
		assert!(Watch::try_from(registration("0x04")).is_err());
	}
}
