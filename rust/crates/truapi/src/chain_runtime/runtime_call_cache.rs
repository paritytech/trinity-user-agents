//! Outputs of runtime calls that depend on nothing but the runtime, shared by
//! every connection to a chain.
//!
//! Each product runtime and the core hold their own chain connection, and each
//! chain client on them fetches the runtime metadata on its own, an answer of
//! 1 to 1.5 MB per client and chain, all at the moment the products start. A
//! repeat is answered from memory, and a caller that arrives while the first
//! fetch is still downloading waits for that fetch instead of starting another.

use std::{
	collections::HashMap,
	sync::{
		Arc, Mutex,
		atomic::{AtomicU64, Ordering},
	},
};

use futures::channel::mpsc;

/// The outcome of a call the cache answers, sent to the caller that made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
	/// The call finished with `output`.
	Done { operation_id: String, output: Arc<Vec<u8>> },
	/// The call will not finish, so the caller asks again.
	Inaccessible { operation_id: String },
}

/// Where a caller receives the answers to its calls.
pub type AnswerSender = mpsc::UnboundedSender<Answer>;

/// Prefix of the operation ids the cache hands out in place of a node.
const CACHED_OPERATION_PREFIX: &str = "truapi-cached-";

/// Whether the cache, not a node, handed out `operation_id`.
pub fn is_cached_operation(operation_id: &str) -> bool {
	operation_id.starts_with(CACHED_OPERATION_PREFIX)
}

/// One runtime call, identified so that every block with the same runtime code
/// shares its output. A chain-head client keys its own metadata cache by the
/// same code hash.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuntimeCall {
	genesis_hash: Vec<u8>,
	code_hash: Vec<u8>,
	function: String,
	call_parameters: Vec<u8>,
}

impl RuntimeCall {
	/// Whether `function` returns the same output at every block of one runtime.
	/// The metadata calls do.
	pub fn depends_only_on_runtime(function: &str) -> bool {
		function.starts_with("Metadata_")
	}

	/// A call of `function` with `call_parameters` on the chain `genesis_hash`,
	/// at a block whose runtime code hashes to `code_hash`.
	pub fn new(
		genesis_hash: &[u8],
		code_hash: Vec<u8>,
		function: String,
		call_parameters: Vec<u8>,
	) -> Self {
		Self { genesis_hash: genesis_hash.to_vec(), code_hash, function, call_parameters }
	}
}

/// What [`RuntimeCallCache::claim`] decided for a caller.
#[derive(Debug, PartialEq, Eq)]
pub enum Claim {
	/// The output is known and was sent to the caller under `operation_id`.
	Answered { operation_id: String },
	/// Another caller is fetching it, and the output reaches this caller under
	/// `operation_id` when that fetch lands.
	Waiting { operation_id: String },
	/// Nobody has it, so this caller fetches it from the node, as fetch number
	/// `fetch`.
	Fetch { fetch: u64 },
	/// Another caller is fetching it and this caller cannot wait, so it fetches
	/// it too. The first output to land answers everyone waiting.
	FetchAlongside,
}

struct Waiter {
	operation_id: String,
	answers: AnswerSender,
}

enum Entry {
	Known(Arc<Vec<u8>>),
	Fetching { fetch: u64, waiters: Vec<Waiter> },
}

/// Runtime call outputs for every chain the runtime talks to, cheap to clone.
#[derive(Clone, Default)]
pub struct RuntimeCallCache {
	entries: Arc<Mutex<HashMap<RuntimeCall, Entry>>>,
	next_operation: Arc<AtomicU64>,
	next_fetch: Arc<AtomicU64>,
}

impl RuntimeCallCache {
	/// Answer `call` from memory, or decide who fetches it: a caller that
	/// `waits` queues behind a fetch in flight, one that does not fetches
	/// alongside it, and the first caller fetches for everyone.
	pub fn claim(&self, call: &RuntimeCall, answers: &AnswerSender, waits: bool) -> Claim {
		let known = {
			let mut entries = self.entries.lock().unwrap();
			match entries.get_mut(call) {
				Some(Entry::Known(output)) => output.clone(),
				Some(Entry::Fetching { .. }) if !waits => return Claim::FetchAlongside,
				Some(Entry::Fetching { waiters, .. }) => {
					let operation_id = self.next_operation_id();
					waiters.push(Waiter {
						operation_id: operation_id.clone(),
						answers: answers.clone(),
					});
					return Claim::Waiting { operation_id };
				},
				None => {
					let fetch = self.next_fetch.fetch_add(1, Ordering::Relaxed);
					entries.insert(call.clone(), Entry::Fetching { fetch, waiters: Vec::new() });
					return Claim::Fetch { fetch };
				},
			}
		};
		Claim::Answered { operation_id: self.answer(known, answers) }
	}

	/// Keep the output of a fetch and send it to everyone waiting for it.
	pub fn fulfil(&self, call: &RuntimeCall, output: Vec<u8>) {
		let output = Arc::new(output);
		let previous =
			self.entries.lock().unwrap().insert(call.clone(), Entry::Known(output.clone()));
		if let Some(Entry::Fetching { waiters, .. }) = previous {
			for waiter in waiters {
				let _ = waiter.answers.unbounded_send(Answer::Done {
					operation_id: waiter.operation_id,
					output: output.clone(),
				});
			}
		}
	}

	/// Drop fetch number `fetch` of `call`, which will not finish. Everyone
	/// waiting for it is told the operation is inaccessible, which a chain-head
	/// client retries. A later fetch of the same call is left alone.
	pub fn abandon(&self, call: &RuntimeCall, fetch: u64) {
		let removed = {
			let mut entries = self.entries.lock().unwrap();
			match entries.get(call) {
				Some(Entry::Fetching { fetch: current, .. }) if *current == fetch => {
					entries.remove(call)
				},
				_ => None,
			}
		};
		if let Some(Entry::Fetching { waiters, .. }) = removed {
			for waiter in waiters {
				let _ = waiter
					.answers
					.unbounded_send(Answer::Inaccessible { operation_id: waiter.operation_id });
			}
		}
	}

	/// Stop waiting on behalf of a caller that stopped its operation.
	pub fn release(&self, operation_id: &str) {
		for entry in self.entries.lock().unwrap().values_mut() {
			if let Entry::Fetching { waiters, .. } = entry {
				waiters.retain(|waiter| waiter.operation_id != operation_id);
			}
		}
	}

	fn answer(&self, output: Arc<Vec<u8>>, answers: &AnswerSender) -> String {
		let operation_id = self.next_operation_id();
		let _ = answers.unbounded_send(Answer::Done { operation_id: operation_id.clone(), output });
		operation_id
	}

	fn next_operation_id(&self) -> String {
		let sequence = self.next_operation.fetch_add(1, Ordering::Relaxed);
		format!("{CACHED_OPERATION_PREFIX}{sequence}")
	}
}
