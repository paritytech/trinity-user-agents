//! The CLI signing host's product workers, run on the core's demand.
//!
//! A worker is a product script the host launches as its own process, on a
//! frame socket of its own that serves the product as a Worker execution, so
//! the core reaches it as it would a worker on any other host. The core's
//! `Start` launches it and its `Stop` kills it. A worker that exits on its own
//! is reported to the core with `worker_exited`, which starts it again while
//! a session still holds it, waiting longer each time it keeps crashing.
//!
//! A worker is a local script, or the one a provider publishes on dotNS,
//! which each launch resolves and fetches before it runs it, unless the
//! archive the provider publishes is already unpacked under the state
//! directory.
//!
//! Every launch, stop and exit is appended to the host's transcript, so a
//! battery can tell the core starting a worker from one already running.

use std::{
	collections::HashMap,
	path::PathBuf,
	process::ExitStatus,
	sync::{Arc, Mutex, MutexGuard, PoisonError, Weak},
};

use tokio::{runtime::Handle, task::JoinHandle};
use truapi::{
	SigningHostRuntime,
	host_logic::worker::{WorkerController, WorkerTransition},
	platform::{ExecutableKind, ProductId},
};

use crate::{
	frame_server, published_worker::PublishedWorkers, script_runner, terminal_ui::UiHandle,
	transcript::Transcript,
};

/// Where a product's worker script comes from.
#[derive(Clone)]
pub enum WorkerSource {
	/// A script on this machine.
	Script(PathBuf),
	/// The worker the product publishes on dotNS, fetched into `cache`.
	Published { workers: Arc<PublishedWorkers>, cache: PathBuf },
}

/// Launches and stops the worker scripts of the products it knows.
pub struct CliWorkerHost {
	/// Where each product's worker script comes from.
	scripts: HashMap<ProductId, WorkerSource>,
	runtime: Weak<SigningHostRuntime>,
	/// Where worker output goes when the host runs a terminal UI.
	ui: Option<UiHandle>,
	transcript: Arc<Transcript>,
	tokio: Handle,
	/// The task serving each product's latest launch. Aborting it kills the
	/// worker.
	running: Mutex<HashMap<ProductId, JoinHandle<()>>>,
	/// Which launch of each product's worker is live, so only its exit is
	/// reported to the core.
	launches: Arc<WorkerLaunchTable>,
}

impl CliWorkerHost {
	/// Run the workers in `scripts` on `runtime`'s demand. Must be called
	/// inside the Tokio runtime the workers are to run on.
	pub fn install(
		runtime: &Arc<SigningHostRuntime>,
		scripts: HashMap<ProductId, WorkerSource>,
		ui: Option<UiHandle>,
		transcript: Arc<Transcript>,
	) -> anyhow::Result<()> {
		let host = Arc::new(Self {
			scripts,
			runtime: Arc::downgrade(runtime),
			ui,
			transcript,
			tokio: Handle::current(),
			running: Mutex::default(),
			launches: Arc::default(),
		});
		anyhow::ensure!(
			runtime.worker_ledger().install_worker_controller(host),
			"the signing host already has a worker controller"
		);
		Ok(())
	}

	fn start(&self, product_id: &ProductId) {
		let Some(script) = self.scripts.get(product_id) else {
			tracing::warn!(%product_id, "the core asked for a worker this host has no script for");
			return;
		};
		let Some(runtime) = self.runtime.upgrade() else {
			return;
		};
		let mut running = lock(&self.running);
		let Some(launch) = self.launches.begin(product_id.clone()) else {
			return;
		};
		self.record(product_id, "Start");
		let task = self.tokio.spawn(serve(
			Launch {
				runtime,
				product_id: product_id.clone(),
				launch,
				launches: self.launches.clone(),
			},
			script.clone(),
			self.ui.clone(),
			self.transcript.clone(),
		));
		running.insert(product_id.clone(), task);
	}

	fn stop(&self, product_id: &ProductId) {
		let mut running = lock(&self.running);
		if self.launches.stop(product_id.clone()).is_none() {
			return;
		}
		if let Some(task) = running.remove(product_id) {
			task.abort();
		}
		self.record(product_id, "Stop");
	}

	fn record(&self, product_id: &ProductId, transition: &str) {
		self.transcript.record(serde_json::json!({
			"kind": "worker",
			"provider": product_id.as_str(),
			"transition": transition,
		}));
	}
}

impl WorkerController for CliWorkerHost {
	fn request_worker_transition(&self, product_id: &ProductId, transition: WorkerTransition) {
		match transition {
			WorkerTransition::Start => self.start(product_id),
			WorkerTransition::Stop => self.stop(product_id),
		}
	}
}

/// One launch of a product's worker, and the table it is live in.
struct Launch {
	runtime: Arc<SigningHostRuntime>,
	product_id: ProductId,
	launch: u64,
	launches: Arc<WorkerLaunchTable>,
}

/// The running workers, whatever a holder panicked with.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Run a product's worker until it exits, record how it ended, and tell the
/// core it stopped without being asked when it was the live launch. A `Stop`
/// aborts this task before it gets that far.
async fn serve(
	launched: Launch,
	source: WorkerSource,
	ui: Option<UiHandle>,
	transcript: Arc<Transcript>,
) {
	let Launch { runtime, product_id, launch, launches } = launched;
	let ran = async {
		let script = script_of(&product_id, source, &transcript).await?;
		run(runtime.clone(), &product_id, script, ui).await
	};
	let exited = match ran.await {
		Ok(status) => serde_json::json!(status.code()),
		Err(error) => {
			tracing::warn!(%product_id, error = %format!("{error:#}"), "the worker could not run");
			serde_json::json!(format!("{error:#}"))
		},
	};
	transcript.record(serde_json::json!({
		"kind": "worker",
		"provider": product_id.as_str(),
		"exited": exited,
	}));
	// End this launch first, so the core's `Start` for it launches a new
	// worker.
	if launches.ended(product_id.clone(), launch) {
		runtime.worker_exited(&product_id);
	}
}

/// The script `source` names for `product_id`'s worker, and the archive it
/// is unpacked in when it is published: the local one, or the published one
/// once fetched, which the transcript records.
async fn script_of(
	product_id: &ProductId,
	source: WorkerSource,
	transcript: &Transcript,
) -> anyhow::Result<(PathBuf, Option<PathBuf>)> {
	match source {
		WorkerSource::Script(script) => Ok((script, None)),
		WorkerSource::Published { workers, cache } => {
			let worker = workers.unpack(product_id, &cache).await?;
			transcript.record(serde_json::json!({
				"kind": "worker",
				"provider": product_id.as_str(),
				"published": worker.script.display().to_string(),
			}));
			Ok((worker.script, Some(worker.archive)))
		},
	}
}

/// Serve `product_id` as a Worker on a frame socket of its own and run
/// `script` against it, sandboxed to `archive` when it is published.
/// Dropping the future kills the script.
async fn run(
	runtime: Arc<SigningHostRuntime>,
	product_id: &ProductId,
	(script, archive): (PathBuf, Option<PathBuf>),
	ui: Option<UiHandle>,
) -> anyhow::Result<ExitStatus> {
	let product = frame_server::ProductSelection::new(product_id.clone(), ExecutableKind::Worker);
	let frames = frame_server::bind(None).await?;
	let frame_url = frames.endpoint().to_string();
	tokio::select! {
		status = script_runner::run_worker(&frame_url, product_id, &script, ui, archive.as_deref()) => status,
		served = frame_server::accept_loop(runtime, product, frames) => {
			Err(served.err().unwrap_or_else(|| anyhow::anyhow!("the worker's frame server stopped")))
		},
	}
}

/// Which launch of each product's worker is the live one, so the host can
/// tell the exit of the current launch from that of one it already replaced
/// or stopped. A launch is a number that only grows.
#[derive(Default)]
struct WorkerLaunchTable {
	state: Mutex<Launches>,
}

#[derive(Default)]
struct Launches {
	next: u64,
	live: HashMap<ProductId, u64>,
}

impl WorkerLaunchTable {
	/// Start a launch of `product_id`'s worker and answer its number, or
	/// `None` while one is live, which the host keeps.
	fn begin(&self, product_id: ProductId) -> Option<u64> {
		let mut state = lock(&self.state);
		if state.live.contains_key(&product_id) {
			return None;
		}
		let launch = state.next;
		state.next += 1;
		state.live.insert(product_id, launch);
		Some(launch)
	}

	/// Record that launch `launch` of `product_id`'s worker exited. Answers
	/// whether it was the live one, whose exit the host reports; an earlier
	/// or stopped launch's exit is ignored.
	fn ended(&self, product_id: ProductId, launch: u64) -> bool {
		let mut state = lock(&self.state);
		if state.live.get(&product_id) != Some(&launch) {
			return false;
		}
		state.live.remove(&product_id);
		true
	}

	/// Stop `product_id`'s live launch, answering its number, so its exit
	/// is not reported. `None` when none is live.
	fn stop(&self, product_id: ProductId) -> Option<u64> {
		lock(&self.state).live.remove(&product_id)
	}
}

#[cfg(test)]
mod tests;
