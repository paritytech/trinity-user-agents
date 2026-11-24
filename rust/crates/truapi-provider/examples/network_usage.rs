//! Runs the embedded light client the way the Android app does, without the
//! app, so its network use can be measured on its own.
//!
//! It adds the nightly build's light-client chains from the bundled catalog,
//! resumes each from a stored database, opens the app's subscriptions and
//! keeps the database saved on the app's schedule. It prints the JSON-RPC
//! traffic per chain at every interval; the network bytes are measured from
//! outside (see `measure.sh` next to the results).
//!
//! Usage:
//!
//! ```text
//! cargo run --release -p truapi-provider --example network_usage --features uniffi -- \
//!   --mode full --subs app-subscriptions-run-D-by-chain.json \
//!   --follows asset-hub,people,people --db-dir db-native --duration 1200 --interval 10
//! ```
//!
//! Modes mirror `benchmarks/network-usage/run.mjs` in smoldot: `none` (chains
//! only), `heads` (+ new heads, finalized heads, runtime version), `storage`
//! (+ the storage subscriptions in `--subs`), `full` (+ the statement filters).
//!
//! At every interval it also reads smoldot's own counters through
//! `EmbeddedChainProvider::metrics` and prints the process's network bytes and
//! each chain's bytes per protocol. `--metrics-out <file>` keeps every reading
//! as one JSON line (`{"t": seconds, "chains": {name: smoldot's answer}}`).

use std::{
	collections::{BTreeMap, HashMap},
	path::PathBuf,
	sync::{Arc, Mutex},
	time::{Duration, Instant},
};

use futures::StreamExt;
use serde_json::{Value, json};
use truapi_provider::{
	EmbeddedChainProvider, StorageClient, StorageClientError, platform::ChainProvider,
};

/// The nightly build's light-client chains (`LightClientChains.of(NIGHTLY)`).
const CHAINS: [(&str, &str); 4] = [
	("relay", "374057be67b355151f271ff70c3db98308c62c8adc48dc6724b6a009a1a014fd"),
	("asset-hub", "4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a"),
	("bulletin", "8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22"),
	("people", "4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48"),
];

/// The app adds chains on threads with this stack: smoldot overflows a
/// default 1 MiB thread stack while adding one.
const THREAD_STACK_BYTES: usize = 8 << 20;

const FIRST_SAVE_DELAY: Duration = Duration::from_secs(30);
const SAVE_INTERVAL: Duration = Duration::from_secs(60);
const SAVE_INTERVAL_MAX: Duration = Duration::from_secs(600);

#[derive(Clone, Copy, PartialEq, PartialOrd)]
enum Mode {
	None,
	Heads,
	Storage,
	Full,
}

struct Options {
	mode: Mode,
	subs: Value,
	follows: Vec<String>,
	db_dir: PathBuf,
	duration: Duration,
	interval: Duration,
	metrics_out: Option<PathBuf>,
}

fn parse_options() -> Options {
	let mut values: HashMap<String, String> = HashMap::new();
	let mut args = std::env::args().skip(1);
	while let Some(flag) = args.next() {
		let name = flag.strip_prefix("--").unwrap_or_else(|| panic!("unexpected argument {flag}"));
		let value = args.next().unwrap_or_else(|| panic!("--{name} needs a value"));
		values.insert(name.to_owned(), value);
	}
	let get = |name: &str, default: &str| values.get(name).cloned().unwrap_or(default.to_owned());
	let mode = match get("mode", "none").as_str() {
		"none" => Mode::None,
		"heads" => Mode::Heads,
		"storage" => Mode::Storage,
		"full" => Mode::Full,
		other => panic!("unknown mode {other}"),
	};
	let subs = match values.get("subs") {
		Some(path) => {
			serde_json::from_str(&std::fs::read_to_string(path).expect("--subs is readable"))
				.expect("--subs is JSON")
		},
		None if mode >= Mode::Storage => panic!("--mode storage and full need --subs"),
		None => Value::Null,
	};
	let follows: Vec<String> = get("follows", "")
		.split(',')
		.filter(|name| !name.is_empty())
		.map(str::to_owned)
		.collect();
	for name in &follows {
		assert!(CHAINS.iter().any(|(chain, _)| chain == name), "unknown --follows chain {name}");
	}
	let seconds = |name: &str, default: &str| {
		Duration::from_secs(
			get(name, default).parse().unwrap_or_else(|_| panic!("--{name} is seconds")),
		)
	};
	Options {
		mode,
		subs,
		follows,
		db_dir: PathBuf::from(get("db-dir", "db-native")),
		duration: seconds("duration", "600"),
		interval: seconds("interval", "30"),
		metrics_out: values.get("metrics-out").map(PathBuf::from),
	}
}

/// One file per chain, named by its hex genesis hash, like the app's
/// `LightClientStorage`.
struct FileStore {
	directory: PathBuf,
}

#[async_trait::async_trait]
impl StorageClient for FileStore {
	// The example builds its provider without a stack, so `stack` is always `None`.
	async fn load(
		&self,
		genesis_hash: [u8; 32],
		_stack: Option<String>,
	) -> Result<Option<String>, StorageClientError> {
		match std::fs::read_to_string(self.directory.join(hex::encode(genesis_hash))) {
			Ok(blob) => Ok(Some(blob)),
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
			Err(error) => Err(StorageClientError::Failed { reason: error.to_string() }),
		}
	}

	async fn save(
		&self,
		genesis_hash: [u8; 32],
		blob: String,
		_stack: Option<String>,
	) -> Result<(), StorageClientError> {
		let failed =
			|error: std::io::Error| StorageClientError::Failed { reason: error.to_string() };
		std::fs::create_dir_all(&self.directory).map_err(failed)?;
		let target = self.directory.join(hex::encode(genesis_hash));
		let written = target.with_extension("tmp");
		std::fs::write(&written, blob).map_err(failed)?;
		std::fs::rename(&written, &target).map_err(failed)
	}
}

/// What one chain's connection has received so far.
#[derive(Default)]
struct ChainCounts {
	frames: u64,
	bytes: u64,
	errors: u64,
	by_method: BTreeMap<String, u64>,
	follow_stops: u64,
}

fn genesis_of(hex_hash: &str) -> [u8; 32] {
	hex::decode(hex_hash).expect("hex").try_into().expect("32 bytes")
}

/// The requests the app sends on `chain` in `mode`, in the order it sends them.
fn requests(options: &Options, chain: &str) -> Vec<(&'static str, Value)> {
	let mut requests = Vec::new();
	if options.mode >= Mode::Heads {
		for method in [
			"chain_subscribeNewHeads",
			"chain_subscribeFinalizedHeads",
			"state_subscribeRuntimeVersion",
		] {
			requests.push((method, json!([])));
		}
	}
	let listed = |field: &str| options.subs[chain][field].as_array().cloned().unwrap_or_default();
	if options.mode >= Mode::Storage {
		for keys in listed("storage") {
			requests.push(("state_subscribeStorage", json!([keys])));
		}
	}
	if options.mode >= Mode::Full {
		for filter in listed("statements") {
			requests.push(("statement_subscribeStatement", json!([filter])));
		}
	}
	for _ in options.follows.iter().filter(|name| name.as_str() == chain) {
		requests.push(("chainHead_v1_follow", json!([true])));
	}
	requests
}

/// Keeps `chainHead_v1_follow` subscriptions alive the way polkadot-api does:
/// unpins finalized blocks except the latest and every pruned block, and
/// follows again after a `stop`. Same rules as `run.mjs`.
struct Follows {
	pending: Vec<u64>,
	pinned: HashMap<String, Option<String>>,
}

impl Follows {
	/// Requests to send in answer to `frame`.
	fn on_frame(&mut self, frame: &Value, counts: &mut ChainCounts) -> Vec<(&'static str, Value)> {
		if let Some(id) = frame["id"].as_u64() {
			if let Some(position) = self.pending.iter().position(|pending| *pending == id) {
				self.pending.remove(position);
				if let Some(subscription) = frame["result"].as_str() {
					self.pinned.insert(subscription.to_owned(), None);
				}
			}
			return Vec::new();
		}
		if frame["method"] != "chainHead_v1_followEvent" {
			return Vec::new();
		}
		let subscription = frame["params"]["subscription"].as_str().unwrap_or_default().to_owned();
		let event = &frame["params"]["result"];
		let hashes = |field: &str| -> Vec<String> {
			event[field]
				.as_array()
				.into_iter()
				.flatten()
				.filter_map(Value::as_str)
				.map(str::to_owned)
				.collect()
		};
		let unpin = |hashes: Vec<String>| -> Vec<(&'static str, Value)> {
			if hashes.is_empty() {
				Vec::new()
			} else {
				vec![("chainHead_v1_unpin", json!([subscription, hashes]))]
			}
		};
		match event["event"].as_str() {
			Some("initialized") => {
				let mut finalized = hashes("finalizedBlockHashes");
				let latest = finalized.pop();
				self.pinned.insert(subscription.clone(), latest);
				unpin(finalized)
			},
			Some("finalized") => {
				let mut finalized = hashes("finalizedBlockHashes");
				let latest = finalized.pop();
				let previous = self.pinned.insert(subscription.clone(), latest).flatten();
				let mut release: Vec<String> = previous.into_iter().collect();
				release.extend(finalized);
				release.extend(hashes("prunedBlockHashes"));
				unpin(release)
			},
			Some("stop") => {
				self.pinned.remove(&subscription);
				counts.follow_stops += 1;
				vec![("chainHead_v1_follow", json!([true]))]
			},
			_ => Vec::new(),
		}
	}
}

async fn run(options: Options) {
	let store = Arc::new(FileStore { directory: options.db_dir.clone() });
	let provider = Arc::new(EmbeddedChainProvider::builder().catalog().storage(store).build());
	let counts: Arc<Mutex<BTreeMap<&str, ChainCounts>>> = Arc::default();
	let mut connections = Vec::new();

	for (name, hex_hash) in CHAINS {
		let genesis = genesis_of(hex_hash);
		let loaded = provider.load_database(genesis).await.expect("the catalog knows the chain");
		let connection = provider.connect(genesis).await.expect("the light client adds the chain");
		println!("{name}: connected, database loaded: {loaded}");
		counts.lock().expect("counts").insert(name, ChainCounts::default());

		let connection: Arc<dyn truapi_provider::platform::JsonRpcConnection> =
			Arc::from(connection);
		let mut next_id = 0u64;
		let mut follows = Follows { pending: Vec::new(), pinned: HashMap::new() };
		let mut send = {
			let connection = connection.clone();
			move |method: &str, params: Value, follows: &mut Follows| {
				next_id += 1;
				if method == "chainHead_v1_follow" {
					follows.pending.push(next_id);
				}
				connection.send(
					json!({"jsonrpc": "2.0", "id": next_id, "method": method, "params": params})
						.to_string(),
				);
			}
		};
		for (method, params) in requests(&options, name) {
			send(method, params, &mut follows);
		}

		let mut responses = connection.responses();
		let counts = counts.clone();
		tokio::spawn(async move {
			while let Some(frame) = responses.next().await {
				let parsed: Value = serde_json::from_str(&frame).unwrap_or(Value::Null);
				let answers = {
					let mut all = counts.lock().expect("counts");
					let chain = all.get_mut(name).expect("chain registered");
					chain.frames += 1;
					chain.bytes += frame.len() as u64;
					if parsed.get("error").is_some() {
						chain.errors += 1;
						eprintln!("{name}: {}", frame.chars().take(300).collect::<String>());
					}
					if let Some(method) = parsed["method"].as_str() {
						*chain.by_method.entry(method.to_owned()).or_default() += 1;
					}
					follows.on_frame(&parsed, chain)
				};
				for (method, params) in answers {
					send(method, params, &mut follows);
				}
			}
			eprintln!("{name}: response stream ended");
		});
		connections.push(connection);
	}

	let saver = {
		let provider = provider.clone();
		tokio::spawn(async move {
			tokio::time::sleep(FIRST_SAVE_DELAY).await;
			let mut interval = SAVE_INTERVAL;
			loop {
				for (name, hex_hash) in CHAINS {
					if let Err(error) = provider.save_database(genesis_of(hex_hash)).await {
						eprintln!("{name}: save failed: {error}");
					}
				}
				tokio::time::sleep(interval).await;
				interval = (interval * 2).min(SAVE_INTERVAL_MAX);
			}
		})
	};

	let started = Instant::now();
	let mut metrics_out = options.metrics_out.as_ref().map(|path| {
		std::fs::OpenOptions::new()
			.create(true)
			.append(true)
			.open(path)
			.expect("--metrics-out is writable")
	});
	while started.elapsed() < options.duration {
		tokio::time::sleep(options.interval.min(options.duration - started.elapsed())).await;
		let elapsed = started.elapsed().as_secs();
		let summary: Vec<String> = counts
			.lock()
			.expect("counts")
			.iter()
			.map(|(name, chain)| {
				format!(
					"{name}: frames={} json={:.2}MB errors={} followStops={} {:?}",
					chain.frames,
					chain.bytes as f64 / 1e6,
					chain.errors,
					chain.follow_stops,
					chain.by_method
				)
			})
			.collect();
		println!("t={elapsed}s {}", summary.join(" | "));
		report_metrics(&provider, elapsed, metrics_out.as_mut()).await;
	}

	saver.abort();
	for (name, hex_hash) in CHAINS {
		if let Err(error) = provider.save_database(genesis_of(hex_hash)).await {
			eprintln!("{name}: final save failed: {error}");
		}
	}
	drop(connections);
}

/// The entries of the metric family `name`: each entry's labels and value.
fn metric_entries<'a>(metrics: &'a Value, name: &str) -> Vec<(&'a Value, f64)> {
	metrics["metrics"]
		.as_array()
		.into_iter()
		.flatten()
		.filter(|family| family["name"] == name)
		.flat_map(|family| family["entries"].as_array().into_iter().flatten())
		.map(|entry| (&entry["labels"], entry["value"].as_f64().unwrap_or(0.0)))
		.collect()
}

fn metric_total(metrics: &Value, name: &str) -> f64 {
	metric_entries(metrics, name).iter().map(|(_, value)| value).sum()
}

/// Prints smoldot's counters since the start: the process's network bytes
/// and connections, then each chain's bytes per protocol (received/sent),
/// biggest first. Appends the raw readings to `out` when given.
async fn report_metrics(
	provider: &EmbeddedChainProvider,
	elapsed: u64,
	out: Option<&mut std::fs::File>,
) {
	let mut readings = serde_json::Map::new();
	for (name, hex_hash) in CHAINS {
		match provider.metrics(genesis_of(hex_hash)).await {
			Ok(Some(answer)) => {
				readings
					.insert(name.to_owned(), serde_json::from_str(&answer).unwrap_or(Value::Null));
			},
			Ok(None) => eprintln!("{name}: not running, no metrics"),
			Err(error) => eprintln!("{name}: metrics failed: {error}"),
		}
	}
	let mb = |bytes: f64| bytes / 1e6;
	if let Some(any) = readings.values().next() {
		println!(
			"t={elapsed}s smoldot: received={:.2}MB sent={:.2}MB connectionsDialed={} handshakesFinished={}",
			mb(metric_total(any, "networkBytesReceivedTotal")),
			mb(metric_total(any, "networkBytesSentTotal")),
			metric_total(any, "networkConnectionsDialedTotal"),
			metric_total(any, "networkConnectionsHandshakesFinishedTotal"),
		);
	}
	for (name, metrics) in &readings {
		let mut by_protocol: BTreeMap<String, (f64, f64)> = BTreeMap::new();
		for (labels, value) in metric_entries(metrics, "networkProtocolBytesTotal") {
			let protocol = labels["protocol"].as_str().unwrap_or("?").to_owned();
			let totals = by_protocol.entry(protocol).or_default();
			if labels["direction"] == "in" { totals.0 += value } else { totals.1 += value }
		}
		let mut ranked: Vec<_> =
			by_protocol.into_iter().filter(|(_, (rx, tx))| rx + tx > 0.0).collect();
		ranked.sort_by(|a, b| (b.1.0 + b.1.1).total_cmp(&(a.1.0 + a.1.1)));
		let listed: Vec<String> = ranked
			.iter()
			.map(|(protocol, (rx, tx))| format!("{protocol}={:.2}/{:.2}", mb(*rx), mb(*tx)))
			.collect();
		println!("t={elapsed}s {name} MB in/out: {}", listed.join(" "));
	}
	if let Some(out) = out {
		use std::io::Write;
		let line = json!({"t": elapsed, "chains": readings});
		writeln!(out, "{line}").expect("--metrics-out is writable");
	}
}

fn main() {
	let options = parse_options();
	println!(
		"mode={} follows={} duration={}s",
		match options.mode {
			Mode::None => "none",
			Mode::Heads => "heads",
			Mode::Storage => "storage",
			Mode::Full => "full",
		},
		if options.follows.is_empty() { "-".to_owned() } else { options.follows.join(",") },
		options.duration.as_secs()
	);
	tokio::runtime::Builder::new_multi_thread()
		.enable_all()
		.thread_stack_size(THREAD_STACK_BYTES)
		.build()
		.expect("tokio runtime")
		.block_on(run(options));
}
