//! The provider set from the provider registry contract on Asset Hub.
//!
//! The registry is a Solidity contract (`contracts/` of the cache repository). It stands in for
//! the service registry of Metanode. An operator registers each cache node: its endpoint id, the
//! base URL of its API, a name, a region and dial hints. The host reads `list()` with a dry run
//! through the runtime API `ReviveApi_call`, at the newest finalized block, as it reads DotNS
//! (`encode_revive_call` and `decode_revive_call_output`). The cache nodes read the same set with
//! the same rules, so the host and the nodes find the same home nodes.
//!
//! The host keeps the last set. A background task reads the registry again every
//! `REFRESH`. When a read fails, the host keeps the set that it has.

use core::time::Duration;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tracing::{info, warn};
use truapi::host_logic::dotns_gateway::{
	VIEW_CALL_ORIGIN, decode_revive_call_output, encode_revive_call,
};

use super::Provider;

/// The time between two reads of the registry.
const REFRESH: Duration = Duration::from_secs(60);
/// The version of the node API that this host speaks. A record with another version is not used.
const PROTOCOL: u16 = 1;
/// The selector of `list()`: the first 4 bytes of `keccak256("list()")`.
const LIST_SELECTOR: [u8; 4] = [0x0f, 0x56, 0x0c, 0xd7];
const RPC_TIMEOUT: Duration = Duration::from_secs(20);

/// One record of the contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
	pub endpoint_id: [u8; 32],
	pub api: String,
	pub name: String,
	pub region: String,
	pub protocol: u16,
	pub active: bool,
}

/// What the host knows of the registry: the providers of the last read, its block, and the
/// error of the last read when it failed.
#[derive(Debug, Clone, Default)]
pub struct RegistryState {
	pub ids: Vec<[u8; 32]>,
	pub providers: Vec<Provider>,
	pub block: Option<u64>,
	pub records: usize,
	pub error: Option<String>,
}

/// The registry contract and the Asset Hub node that the host asks.
pub struct Registry {
	http: reqwest::Client,
	rpc: String,
	contract: [u8; 20],
	state: Arc<Mutex<RegistryState>>,
}

impl Registry {
	/// The registry at `contract` (0x and 40 hex digits), read through the JSON-RPC URL `rpc`. A
	/// `ws://` or `wss://` URL becomes `http://` or `https://`: the public Asset Hub nodes answer
	/// both on one URL.
	pub fn new(contract: &str, rpc: &str) -> Result<Self, String> {
		let bytes = hex::decode(contract.trim().trim_start_matches("0x"))
			.map_err(|_| format!("the registry address {contract} is not hex"))?;
		let contract = <[u8; 20]>::try_from(bytes)
			.map_err(|_| format!("the registry address {contract} does not have 20 bytes"))?;
		let rpc = http_url(rpc);
		let http = reqwest::Client::builder()
			.timeout(RPC_TIMEOUT)
			.build()
			.map_err(|error| format!("no HTTP client for the registry: {error}"))?;
		Ok(Self { http, rpc, contract, state: Arc::default() })
	}

	pub fn describe(&self) -> String {
		format!("registry 0x{} at {}", hex::encode(self.contract), self.rpc)
	}

	pub fn state(&self) -> RegistryState {
		self.state.lock().expect("registry state mutex poisoned").clone()
	}

	/// Read the registry now, and then every `REFRESH` in a background task. The task stops when
	/// there is no Tokio runtime, and then the host has no providers.
	pub fn start(self: &Arc<Self>) {
		let Ok(runtime) = tokio::runtime::Handle::try_current() else {
			warn!("no Tokio runtime, so the host does not read the provider registry");
			return;
		};
		let registry = Arc::clone(self);
		runtime.spawn(async move {
			loop {
				registry.refresh().await;
				tokio::time::sleep(REFRESH).await;
			}
		});
	}

	/// Read the registry once, and keep the result. A failed read keeps the providers that the
	/// host has.
	pub async fn refresh(&self) {
		let result = self.read().await;
		let mut state = self.state.lock().expect("registry state mutex poisoned");
		match result {
			Ok((block, records)) => {
				let (ids, providers) = usable(&records);
				if state.block.is_none() || state.records != records.len() || state.ids != ids {
					info!(block, providers = providers.len(), "{}", self.describe());
				}
				*state = RegistryState {
					ids,
					providers,
					block: Some(block),
					records: records.len(),
					error: None,
				};
			},
			Err(error) => {
				warn!(%error, "{}", self.describe());
				state.error = Some(error);
			},
		}
	}

	/// The records at the newest finalized block, and the number of that block.
	async fn read(&self) -> Result<(u64, Vec<Record>), String> {
		let head = self.call("chain_getFinalizedHead", json!([])).await?;
		let head = head.as_str().ok_or("the finalized head is not a string")?.to_string();
		let header = self.call("chain_getHeader", json!([head])).await?;
		let number = header["number"].as_str().ok_or("the header has no number")?;
		let block = u64::from_str_radix(number.trim_start_matches("0x"), 16)
			.map_err(|_| format!("the block number {number} is not hex"))?;
		let data = view(&self.http, &self.rpc, &self.contract, &LIST_SELECTOR, Some(&head)).await?;
		// A call to an address without code succeeds and returns nothing.
		if data.is_empty() {
			return Err("the registry address has no contract".into());
		}
		Ok((block, decode_list(&data)?))
	}

	async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
		call(&self.http, &self.rpc, method, params).await
	}
}

/// The JSON-RPC URL for `rpc`. A `ws://` or `wss://` URL becomes `http://` or `https://`: the
/// public Asset Hub nodes answer both on one URL.
pub fn http_url(rpc: &str) -> String {
	if let Some(rest) = rpc.strip_prefix("wss://") {
		format!("https://{rest}")
	} else if let Some(rest) = rpc.strip_prefix("ws://") {
		format!("http://{rest}")
	} else {
		rpc.to_string()
	}
}

/// The result of one JSON-RPC call of `method` with `params` at the HTTP URL `rpc`.
async fn call(
	http: &reqwest::Client,
	rpc: &str,
	method: &str,
	params: Value,
) -> Result<Value, String> {
	let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
	let reply: Value = http
		.post(rpc)
		.json(&body)
		.send()
		.await
		.and_then(reqwest::Response::error_for_status)
		.map_err(|error| format!("{method}: {error}"))?
		.json()
		.await
		.map_err(|error| format!("{method}: {error}"))?;
	if let Some(error) = reply.get("error") {
		return Err(format!("{method}: {error}"));
	}
	reply.get("result").cloned().ok_or_else(|| format!("{method}: no result"))
}

/// The return data of a dry run of the contract call `input` on `contract`, through the runtime
/// API `ReviveApi_call` at the block `at`, or at the best block without `at`.
pub async fn view(
	http: &reqwest::Client,
	rpc: &str,
	contract: &[u8; 20],
	input: &[u8],
	at: Option<&str>,
) -> Result<Vec<u8>, String> {
	let args = format!("0x{}", hex::encode(encode_revive_call(&VIEW_CALL_ORIGIN, contract, input)));
	let params = match at {
		Some(at) => json!(["ReviveApi_call", args, at]),
		None => json!(["ReviveApi_call", args]),
	};
	let output = call(http, rpc, "state_call", params).await?;
	let output = output.as_str().ok_or("state_call gave no hex")?;
	let output = hex::decode(output.trim_start_matches("0x"))
		.map_err(|_| "state_call gave no hex".to_string())?;
	decode_revive_call_output(&output).map_err(|error| error.to_string())
}

/// The providers that the host uses: active records with the protocol of this host, and an
/// `https://` API URL (`http://` only on the loopback, for a local network). The cache nodes use
/// the same rules. All of them rank the home nodes, so the answer has their endpoint ids too.
pub fn usable(records: &[Record]) -> (Vec<[u8; 32]>, Vec<Provider>) {
	let providers: Vec<Provider> = records
		.iter()
		.filter(|record| record.active && record.protocol == PROTOCOL && api_allowed(&record.api))
		.map(|record| {
			let text = |value: &str| (!value.is_empty()).then(|| value.to_string());
			Provider {
				id: record.endpoint_id,
				url: record.api.trim_end_matches('/').to_string(),
				name: text(&record.name),
				region: text(&record.region),
			}
		})
		.collect();
	(providers.iter().map(|provider| provider.id).collect(), providers)
}

fn api_allowed(api: &str) -> bool {
	let Ok(url) = reqwest::Url::parse(api) else {
		return false;
	};
	match url.scheme() {
		"https" => url.host().is_some(),
		"http" => matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")),
		_ => false,
	}
}

/// ABI data, read with bounds checks. An offset of a dynamic value counts from the start of the
/// value that holds it.
struct Abi<'a> {
	data: &'a [u8],
}

impl Abi<'_> {
	fn word(&self, at: usize) -> Result<&[u8], String> {
		at.checked_add(32)
			.filter(|end| *end <= self.data.len())
			.map(|end| &self.data[at..end])
			.ok_or_else(|| format!("the ABI data is too short at byte {at}"))
	}

	/// An unsigned integer that fits in a `usize`, for example an offset or a length.
	fn number(&self, at: usize) -> Result<usize, String> {
		let word = self.word(at)?;
		if word[..24].iter().any(|byte| *byte != 0) {
			return Err(format!("a number too large at byte {at}"));
		}
		let value = u64::from_be_bytes(word[24..].try_into().expect("8 bytes; qed"));
		usize::try_from(value).map_err(|_| format!("a number too large at byte {at}"))
	}

	fn pointer(&self, base: usize, at: usize) -> Result<usize, String> {
		base.checked_add(self.number(at)?)
			.ok_or_else(|| format!("an offset out of range at byte {at}"))
	}

	/// The `bytes` or `string` value at `at`: a length word, then the bytes.
	fn bytes(&self, at: usize) -> Result<&[u8], String> {
		let length = self.number(at)?;
		let start = at + 32;
		let end = start
			.checked_add(length)
			.filter(|end| *end <= self.data.len())
			.ok_or_else(|| format!("a value longer than the data at byte {at}"))?;
		Ok(&self.data[start..end])
	}

	fn string(&self, at: usize) -> Result<String, String> {
		String::from_utf8(self.bytes(at)?.to_vec())
			.map_err(|_| format!("a string that is not UTF-8 at byte {at}"))
	}
}

/// The value of return data that is one `bytes` value.
pub fn decode_bytes(data: &[u8]) -> Result<Vec<u8>, String> {
	let abi = Abi { data };
	abi.bytes(abi.pointer(0, 0)?).map(<[u8]>::to_vec)
}

/// The records in the return data of `list()`: `abi.encode(Provider[])`, where `Provider` is
/// `(bytes32 operator, bytes32 endpointId, string api, string name, string region,
/// string[] dialHints, uint16 protocol, uint64 registeredAt, uint64 updatedAt, bool active)`. The
/// host does not dial the nodes, so it does not read the dial hints.
pub fn decode_list(data: &[u8]) -> Result<Vec<Record>, String> {
	let abi = Abi { data };
	let array = abi.pointer(0, 0)?;
	let count = abi.number(array)?;
	// A record takes at least 10 words.
	if count > data.len() / 320 {
		return Err(format!("{count} records in {} bytes", data.len()));
	}
	let base = array + 32;
	(0..count)
		.map(|i| {
			let record = abi.pointer(base, base + 32 * i)?;
			let field = |n: usize| record + 32 * n;
			let protocol = abi.number(field(6))?;
			Ok(Record {
				endpoint_id: abi.word(field(1))?.try_into().expect("32 bytes; qed"),
				api: abi.string(abi.pointer(record, field(2))?)?,
				name: abi.string(abi.pointer(record, field(3))?)?,
				region: abi.string(abi.pointer(record, field(4))?)?,
				protocol: u16::try_from(protocol)
					.map_err(|_| format!("a protocol of {protocol}"))?,
				active: match abi.number(field(9))? {
					0 => false,
					1 => true,
					other => return Err(format!("a bool of {other}")),
				},
			})
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	/// A read of `list()` on a local network, from the cache repository
	/// (`contracts/vectors/list.json`).
	fn vector() -> Value {
		serde_json::from_str(include_str!("testdata/registry-list.json")).expect("valid JSON")
	}

	fn bytes(value: &Value) -> Vec<u8> {
		hex::decode(value.as_str().expect("hex").trim_start_matches("0x")).expect("hex")
	}

	#[test]
	fn the_call_is_the_call_of_the_cache_nodes() {
		let vector = vector();
		let contract: [u8; 20] = bytes(&vector["contract"]).try_into().expect("20 bytes");
		assert_eq!(
			encode_revive_call(&VIEW_CALL_ORIGIN, &contract, &LIST_SELECTOR),
			bytes(&vector["args"])
		);
	}

	#[test]
	fn decodes_a_read_of_the_chain() {
		let vector = vector();
		let data = decode_revive_call_output(&bytes(&vector["output"])).expect("a result");
		assert_eq!(data, bytes(&vector["return"]));
		let records = decode_list(&data).expect("records");
		let expected = vector["providers"].as_array().expect("providers");
		assert_eq!(records.len(), expected.len());
		for (record, expected) in records.iter().zip(expected) {
			assert_eq!(format!("0x{}", hex::encode(record.endpoint_id)), expected["endpointId"]);
			assert_eq!(record.api, expected["api"]);
			assert_eq!(record.name, expected["name"]);
			assert_eq!(record.region, expected["region"]);
			assert_eq!(record.active, expected["active"]);
		}
		// The set has A and C. B is inactive.
		let (ids, providers) = usable(&records);
		assert_eq!(ids.len(), 2);
		let names: Vec<_> = providers.iter().filter_map(|provider| provider.name.clone()).collect();
		assert_eq!(names, ["A", "C"]);
	}

	#[test]
	fn applies_the_rules_of_the_cache_nodes() {
		let record = Record {
			endpoint_id: [7; 32],
			api: "https://cache.example.org/".into(),
			name: "N".into(),
			region: String::new(),
			protocol: PROTOCOL,
			active: true,
		};
		for (api, allowed) in [
			("https://cache.example.org/", true),
			("http://127.0.0.1:18081", true),
			("http://localhost:18081", true),
			("http://cache.example.org", false),
			("ftp://cache.example.org", false),
		] {
			let record = Record { api: api.into(), ..record.clone() };
			assert_eq!(usable(&[record]).1.len(), usize::from(allowed), "{api}");
		}
		assert!(usable(&[Record { active: false, ..record.clone() }]).1.is_empty());
		assert!(usable(&[Record { protocol: PROTOCOL + 1, ..record.clone() }]).1.is_empty());
		assert_eq!(usable(&[record]).1[0].url, "https://cache.example.org");
	}

	#[test]
	fn refuses_broken_data() {
		let data = bytes(&vector()["return"]);
		assert!(decode_list(&data[..data.len() / 2]).is_err());
		assert_eq!(decode_list(&[0u8; 64]), Ok(Vec::new()));
	}

	#[test]
	fn a_websocket_url_becomes_http() {
		let registry =
			Registry::new("0x5FbDB2315678afecb367f032d93F642f64180aa3", "wss://example.org/x")
				.expect("valid");
		assert_eq!(registry.rpc, "https://example.org/x");
		assert!(Registry::new("0x1234", "http://x").is_err());
	}
}
