//! The CLI signing host's product workers, run on the core's demand.
//!
//! A worker is a product script the host launches as its own process, on a
//! frame socket of its own that serves the product as a Worker execution, so
//! the core reaches it as it would a worker on any other host. The core's
//! `Start` launches it and its `Stop` kills it. A worker that exits on its own
//! is reported to the core with `worker_exited`, which starts it again while
//! a session still holds it, waiting longer each time it keeps crashing.
//!
//! Every launch, stop and exit is appended to the host's transcript, so a
//! battery can tell the core starting a worker from one already running.

use std::{
	collections::HashMap,
	path::{Path, PathBuf},
	process::ExitStatus,
	sync::{Arc, Mutex, MutexGuard, PoisonError, Weak},
};

use tokio::{runtime::Handle, task::JoinHandle};
use truapi::{
	SigningHostRuntime,
	host_logic::worker::{WorkerController, WorkerTransition},
	platform::{ExecutableKind, ProductId},
};

use crate::{frame_server, script_runner, terminal_ui::UiHandle, transcript::Transcript};

/// The task serving each running worker, by product, with the id it was
/// launched under.
type Running = HashMap<ProductId, (u64, JoinHandle<()>)>;

/// Launches and stops the worker scripts of the products it knows.
pub struct CliWorkerHost {
	/// The script each product's worker runs.
	scripts: HashMap<ProductId, PathBuf>,
	runtime: Weak<SigningHostRuntime>,
	/// Where worker output goes when the host runs a terminal UI.
	ui: Option<UiHandle>,
	transcript: Arc<Transcript>,
	tokio: Handle,
	/// The task serving each running worker, with the id it was launched
	/// under. Aborting it kills the worker.
	running: Arc<Mutex<Running>>,
	/// The id the next launch takes.
	next_launch: std::sync::atomic::AtomicU64,
}

impl CliWorkerHost {
	/// Run the workers in `scripts` on `runtime`'s demand. Must be called
	/// inside the Tokio runtime the workers are to run on.
	pub fn install(
		runtime: &Arc<SigningHostRuntime>,
		scripts: HashMap<ProductId, PathBuf>,
		ui: Option<UiHandle>,
		transcript: Arc<Transcript>,
	) -> anyhow::Result<()> {
		let host = Arc::new(Self {
			scripts,
			runtime: Arc::downgrade(runtime),
			ui,
			transcript,
			tokio: Handle::current(),
			running: Arc::default(),
			next_launch: std::sync::atomic::AtomicU64::new(0),
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
		if running.get(product_id).is_some_and(|(_, task)| !task.is_finished()) {
			return;
		}
		self.record(product_id, "Start");
		let launch = self.next_launch.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
		let task = self.tokio.spawn(serve(
			Launch {
				runtime,
				product_id: product_id.clone(),
				launch,
				running: self.running.clone(),
			},
			script.clone(),
			self.ui.clone(),
			self.transcript.clone(),
		));
		running.insert(product_id.clone(), (launch, task));
	}

	fn stop(&self, product_id: &ProductId) {
		let Some((_, task)) = lock(&self.running).remove(product_id) else {
			return;
		};
		task.abort();
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

/// One launch of a product's worker, and where it is held while it runs.
struct Launch {
	runtime: Arc<SigningHostRuntime>,
	product_id: ProductId,
	launch: u64,
	running: Arc<Mutex<Running>>,
}

/// The running workers, whatever a holder panicked with.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Run a product's worker until it exits, record how it ended, and tell the
/// core it stopped without being asked. A `Stop` aborts this task before it
/// gets that far, so only an exit of the worker's own is reported.
async fn serve(
	launched: Launch,
	script: PathBuf,
	ui: Option<UiHandle>,
	transcript: Arc<Transcript>,
) {
	let Launch { runtime, product_id, launch, running } = launched;
	let exited = match run(runtime.clone(), &product_id, &script, ui).await {
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
	// Let go of this launch first, so the core's `Start` for it launches a
	// new worker; a later launch in its place is left alone.
	{
		let mut running = lock(&running);
		if running.get(&product_id).is_some_and(|(held, _)| *held == launch) {
			running.remove(&product_id);
		}
	}
	runtime.worker_exited(&product_id);
}

/// Serve `product_id` as a Worker on a frame socket of its own and run
/// `script` against it. Dropping the future kills the script.
async fn run(
	runtime: Arc<SigningHostRuntime>,
	product_id: &ProductId,
	script: &Path,
	ui: Option<UiHandle>,
) -> anyhow::Result<ExitStatus> {
	let product = frame_server::ProductSelection::new(product_id.clone(), ExecutableKind::Worker);
	let frames = frame_server::bind(None).await?;
	let frame_url = frames.endpoint().to_string();
	tokio::select! {
		status = script_runner::run_worker(&frame_url, product_id, script, ui) => status,
		served = frame_server::accept_loop(runtime, product, frames) => {
			Err(served.err().unwrap_or_else(|| anyhow::anyhow!("the worker's frame server stopped")))
		},
	}
}
