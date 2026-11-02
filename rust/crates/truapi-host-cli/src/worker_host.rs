//! The CLI signing host's product workers, run on the core's demand.
//!
//! A worker is a product script the host launches as its own process, on a
//! frame socket of its own that serves the product as a Worker execution, so
//! the core reaches it as it would a worker on any other host. The core's
//! `Start` launches it and its `Stop` kills it. The core is not told when a
//! worker exits on its own, so it stays stopped while a session still holds
//! it, and starts again only once those sessions end and the core asks anew.
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
	platform::{ProductExecutionKind, ProductId},
};

use crate::{frame_server, script_runner, terminal_ui::UiHandle, transcript::Transcript};

/// Launches and stops the worker scripts of the products it knows.
pub struct CliWorkerHost {
	/// The script each product's worker runs.
	scripts: HashMap<ProductId, PathBuf>,
	runtime: Weak<SigningHostRuntime>,
	/// Where worker output goes when the host runs a terminal UI.
	ui: Option<UiHandle>,
	transcript: Arc<Transcript>,
	tokio: Handle,
	/// The task serving each running worker. Aborting it kills the worker.
	running: Mutex<HashMap<ProductId, JoinHandle<()>>>,
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
			running: Mutex::new(HashMap::new()),
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
		let mut running = self.lock_running();
		if running.get(product_id).is_some_and(|task| !task.is_finished()) {
			return;
		}
		self.record(product_id, "Start");
		let task = self.tokio.spawn(serve(
			runtime,
			product_id.clone(),
			script.clone(),
			self.ui.clone(),
			self.transcript.clone(),
		));
		running.insert(product_id.clone(), task);
	}

	fn stop(&self, product_id: &ProductId) {
		let Some(task) = self.lock_running().remove(product_id) else {
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

	fn lock_running(&self) -> MutexGuard<'_, HashMap<ProductId, JoinHandle<()>>> {
		self.running.lock().unwrap_or_else(PoisonError::into_inner)
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

/// Run `product_id`'s worker until it exits, and record how it ended.
async fn serve(
	runtime: Arc<SigningHostRuntime>,
	product_id: ProductId,
	script: PathBuf,
	ui: Option<UiHandle>,
	transcript: Arc<Transcript>,
) {
	let exited = match run(runtime, &product_id, &script, ui).await {
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
}

/// Serve `product_id` as a Worker on a frame socket of its own and run
/// `script` against it. Dropping the future kills the script.
async fn run(
	runtime: Arc<SigningHostRuntime>,
	product_id: &ProductId,
	script: &Path,
	ui: Option<UiHandle>,
) -> anyhow::Result<ExitStatus> {
	let product =
		frame_server::ProductSelection::new(product_id.to_string(), ProductExecutionKind::Worker)?;
	let frames = frame_server::bind(None).await?;
	let frame_url = frames.endpoint().to_string();
	tokio::select! {
		status = script_runner::run_worker(&frame_url, product_id, script, ui) => status,
		served = frame_server::accept_loop(runtime, product, frames) => {
			Err(served.err().unwrap_or_else(|| anyhow::anyhow!("the worker's frame server stopped")))
		},
	}
}
