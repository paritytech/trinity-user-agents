use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::host_logic::worker::{WorkerDemandObserver, WorkerTransition};
use crate::platform::{ProductContext, ProductExecutionKind};
use crate::store::{ProductWorkerRecord, RuntimeStore, WorkerReason};
use crate::subscription::Spawner;

use super::callbacks::{NativeChatCallbacks, NativePocketCallbacks};
use super::errors::HostRejection;
use super::runtime::{NativeProductExecution, NativeTrUApiHostRuntime};
use super::storage::NativeStorage;

/// Content-addressed files prepared by the host's bundle adapter.
#[derive(Debug, Clone, uniffi::Record)]
pub struct WorkerBundle {
    /// Hash of the installed bundle, independent of its display version.
    pub content_hash: Vec<u8>,
    /// Manifest bytes resolved with this bundle.
    pub manifest: Vec<u8>,
    /// Local directory containing the worker executable.
    pub local_path: String,
}

/// OS engine and content fetching only; Rust owns worker lifecycle decisions.
#[uniffi::export(rust, foreign)]
#[async_trait::async_trait]
pub trait NativeWorkerEngineHost: Send + Sync {
    /// Per-product Chat adapter, absent when the host has no Chat modality.
    fn chat_callbacks(&self, product_id: String) -> Option<Arc<dyn NativeChatCallbacks>>;
    /// Per-product Pocket adapter, absent when the host has no Pocket modality.
    fn pocket_callbacks(&self, product_id: String) -> Option<Arc<dyn NativePocketCallbacks>>;
    /// Start an engine attached to the supplied core execution.
    fn start_worker(&self, product_id: String, execution: Arc<NativeProductExecution>, bundle: WorkerBundle) -> Result<(), HostRejection>;
    /// Tear down the product's engine before its execution is replaced.
    async fn stop_worker(&self, product_id: String) -> Result<(), HostRejection>;
    /// Resolve the current bundle, or open a previously downloaded content hash.
    async fn fetch_worker_bundle(&self, product_id: String, content_hash: Option<Vec<u8>>) -> Result<WorkerBundle, HostRejection>;
}

/// Durable worker reference requested by native UI.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum WorkerIntentAction {
    /// Retain an integration across application restarts.
    Add,
    /// Remove only the selected integration reference.
    Remove,
}

/// Native modality keeping a product worker alive.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum WorkerModality {
    /// Installed product chat.
    Chat,
    /// One installed Pocket card.
    Card {
        /// Stable identifier of the retained card.
        card_id: String,
    },
}

struct EngineSetup {
    engine: Arc<dyn NativeWorkerEngineHost>,
}

struct RunningWorker {
    execution: Arc<NativeProductExecution>,
    content_hash: Vec<u8>,
}

/// One worker per product, derived from durable rows and live core references.
pub struct NativeWorkers {
    storage: Arc<NativeStorage>,
    host: OnceLock<Weak<NativeTrUApiHostRuntime>>,
    own: OnceLock<Weak<Self>>,
    setup: OnceLock<EngineSetup>,
    demand: Mutex<HashSet<String>>,
    running: futures::lock::Mutex<HashMap<String, RunningWorker>>,
    suspended: AtomicBool,
    generation: AtomicU64,
    spawner: Spawner,
    retry_after: Mutex<HashMap<String, std::time::Instant>>,
}

impl NativeWorkers {
    /// Worker state starts suspended until a wallet has activated.
    pub fn new(storage: Arc<NativeStorage>, spawner: Spawner) -> Arc<Self> {
        let workers = Arc::new(Self {
            storage, host: OnceLock::new(), own: OnceLock::new(), setup: OnceLock::new(),
            demand: Mutex::new(HashSet::new()), running: futures::lock::Mutex::new(HashMap::new()),
            suspended: AtomicBool::new(true), generation: AtomicU64::new(0), spawner,
            retry_after: Mutex::new(HashMap::new()),
        });
        workers.own.set(Arc::downgrade(&workers)).expect("new worker supervisor");
        workers
    }

    /// Attach the facade without creating an ownership cycle.
    pub fn attach(&self, host: Weak<NativeTrUApiHostRuntime>) {
        assert!(self.host.set(host).is_ok());
    }

    /// Install the host engine adapter before restoring workers.
    pub fn set_engine(&self, engine: Arc<dyn NativeWorkerEngineHost>) -> bool {
        self.setup.set(EngineSetup { engine }).is_ok()
    }

    /// Persist a native integration intent and reconcile the one product worker.
    pub async fn intent(&self, product: String, action: WorkerIntentAction, modality: WorkerModality) -> Result<(), HostRejection> {
        let product = crate::platform::normalize_product_identifier(&product).map_err(rejection)?;
        let store = self.storage.current()?.for_product(&product).map_err(rejection)?;
        let reason = match modality {
            WorkerModality::Chat => WorkerReason::Chat,
            WorkerModality::Card { card_id } => WorkerReason::Pocket { card_id },
        };
        let mut running = self.running.lock().await;
        match action {
            WorkerIntentAction::Add => {
                self.ensure_worker(&store, &product).await?;
                store.add_worker_reason(product.clone(), reason, now_millis()?).await.map_err(rejection)?;
            }
            WorkerIntentAction::Remove => store.remove_worker_reason(product.clone(), reason).await.map_err(rejection)?,
        }
        self.reconcile(&store, &mut running, &product, false).await
    }

    /// Restore account workers and check bundles at launch or after foregrounding.
    pub async fn resume(&self) -> Result<(), HostRejection> {
        let was_suspended = self.suspended.swap(false, Ordering::AcqRel);
        if was_suspended {
            self.monitor();
        }
        self.update(true).await
    }

    /// Native background work retains the same worker as product operations.
    pub async fn begin_operation(&self, product: String, label: Option<String>) -> Result<crate::store::WorkerOperationRecord, HostRejection> {
        let product = crate::platform::normalize_product_identifier(&product).map_err(rejection)?;
        let store = self.storage.current()?.for_product(&product).map_err(rejection)?;
        let mut running = self.running.lock().await;
        self.ensure_worker(&store, &product).await?;
        let operation = store.begin_worker_operation(product.clone(), label, now_millis()?).await.map_err(rejection)?;
        self.reconcile(&store, &mut running, &product, false).await?;
        Ok(operation)
    }

    /// Removing an operation also applies any downloaded update that was deferred.
    pub async fn end_operation(&self, product: String, operation_id: u32) -> Result<(), HostRejection> {
        let store = self.storage.current()?.for_product(&product).map_err(rejection)?;
        let mut running = self.running.lock().await;
        store.end_worker_operation(product.clone(), operation_id).await.map_err(rejection)?;
        self.reconcile(&store, &mut running, &product, false).await
    }

    /// Stop engines without deleting this account's reasons or operations.
    pub async fn suspend(&self) -> Result<(), HostRejection> {
        self.suspended.store(true, Ordering::Release);
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.retry_after.lock().expect("worker retry mutex poisoned").clear();
        let mut running = self.running.lock().await;
        let products: Vec<_> = running.keys().cloned().collect();
        let mut failure = None;
        for product in products {
            if let Err(error) = self.stop(&mut running, &product).await {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }

    /// Stop a product engine before deleting its durable references.
    pub async fn stop_product(&self, product: &str) -> Result<(), HostRejection> {
        let mut running = self.running.lock().await;
        self.stop(&mut running, product).await
    }

    /// Reconcile operation changes using the same durable and live references.
    pub fn changed(&self, product: String) {
        let Some(workers) = self.own.get().and_then(Weak::upgrade) else { return };
        (self.spawner)(Box::pin(async move {
            let result = async {
                if workers.suspended.load(Ordering::Acquire) { return Ok(()) }
                let store = workers.storage.current()?.for_product(&product).map_err(rejection)?;
                let mut running = workers.running.lock().await;
                workers.reconcile(&store, &mut running, &product, false).await
            }.await;
            if let Err(error) = result { workers.report(error); }
        }));
    }

    /// Periodic bundle checks share reconciliation with explicit native intents.
    pub async fn update(&self, force_check: bool) -> Result<(), HostRejection> {
        if self.suspended.load(Ordering::Acquire) { return Ok(()) }
        let store = self.storage.current()?;
        let mut products: HashSet<_> = store.workers().await.map_err(rejection)?.into_iter().map(|worker| worker.product_id).collect();
        products.extend(self.demand.lock().expect("worker demand mutex poisoned").iter().cloned());
        let mut running = self.running.lock().await;
        let mut failure = None;
        for product in products {
            if let Err(error) = self.reconcile(&store, &mut running, &product, force_check).await {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }

    async fn ensure_worker(&self, store: &RuntimeStore, product: &str) -> Result<ProductWorkerRecord, HostRejection> {
        if let Some(worker) = store.workers().await.map_err(rejection)?.into_iter().find(|worker| worker.product_id == product) {
            return Ok(worker)
        }
        let setup = self.setup.get().ok_or_else(|| rejection("worker engine is unavailable"))?;
        let bundle = setup.engine.fetch_worker_bundle(product.to_string(), None).await?;
        validate_bundle(&bundle)?;
        store.ensure_product(product.to_string()).await.map_err(rejection)?;
        let worker = ProductWorkerRecord { product_id: product.to_string(), content_hash: bundle.content_hash, pending_hash: None, manifest: bundle.manifest, checked_at: now_millis()? / 1000, failure_count: 0, last_error: None };
        store.save_worker(worker.clone()).await.map_err(rejection)?;
        Ok(worker)
    }

    fn monitor(&self) {
        let weak = self.own.get().expect("worker supervisor attached").clone();
        let generation = self.generation.load(Ordering::Acquire);
        (self.spawner)(Box::pin(async move {
            loop {
                futures_timer::Delay::new(core::time::Duration::from_secs(300)).await;
                let Some(workers) = weak.upgrade() else { return };
                if workers.suspended.load(Ordering::Acquire) || generation != workers.generation.load(Ordering::Acquire) { return }
                if let Err(error) = workers.update(false).await { workers.report(error); }
            }
        }));
    }

    async fn reconcile(&self, store: &RuntimeStore, running: &mut HashMap<String, RunningWorker>, product: &str, force_check: bool) -> Result<(), HostRejection> {
        if self.retry_after.lock().expect("worker retry mutex poisoned").get(product).is_some_and(|deadline| *deadline > std::time::Instant::now()) {
            return Ok(())
        }
        let store = store.for_product(product).map_err(rejection)?;
        let result = self.reconcile_product(&store, running, product, force_check).await;
        if let Err(error) = &result {
            self.retry(&store, product, error.to_string()).await?;
        }
        result
    }

    async fn retry(&self, store: &RuntimeStore, product: &str, reason: String) -> Result<(), HostRejection> {
        let Some(mut record) = store.workers().await.map_err(rejection)?.into_iter().find(|record| record.product_id == product) else { return Ok(()) };
        record.failure_count = record.failure_count.saturating_add(1);
        record.last_error = Some(reason);
        let delay = core::time::Duration::from_secs(1u64 << record.failure_count.min(6));
        store.save_worker(record).await.map_err(rejection)?;
        self.retry_after.lock().expect("worker retry mutex poisoned").insert(product.to_string(), std::time::Instant::now() + delay);
        let weak = self.own.get().expect("worker supervisor attached").clone();
        let generation = self.generation.load(Ordering::Acquire);
        let product = product.to_string();
        (self.spawner)(Box::pin(async move {
            futures_timer::Delay::new(delay).await;
            if let Some(workers) = weak.upgrade()
                && generation == workers.generation.load(Ordering::Acquire) {
                workers.changed(product);
            }
        }));
        Ok(())
    }

    async fn reconcile_product(&self, store: &RuntimeStore, running: &mut HashMap<String, RunningWorker>, product: &str, force_check: bool) -> Result<(), HostRejection> {
        if self.suspended.load(Ordering::Acquire) { return Ok(()) }
        let operations = store.worker_operations().await.map_err(rejection)?.iter().any(|operation| operation.product_id == product);
        let live = self.demand.lock().expect("worker demand mutex poisoned").contains(product);
        let durable = !store.worker_reasons(product.to_string()).await.map_err(rejection)?.is_empty();
        if !operations && !live && !durable {
            self.stop(running, product).await?;
            store.remove_worker_if_unused(product.to_string(), false).await.map_err(rejection)?;
            return Ok(())
        }
        let setup = self.setup.get().ok_or_else(|| rejection("worker engine is unavailable"))?;
        let mut worker = self.ensure_worker(store, product).await?;
        let now = now_millis()? / 1000;
        if force_check || now.saturating_sub(worker.checked_at) >= 86_400 {
            match setup.engine.fetch_worker_bundle(product.to_string(), None).await {
                Ok(bundle) => {
                    validate_bundle(&bundle)?;
                    if bundle.content_hash != worker.content_hash { worker.pending_hash = Some(bundle.content_hash); }
                    worker.checked_at = now;
                    worker.last_error = None;
                }
                Err(error) => {
                    worker.last_error = Some(error.to_string());
                    worker.checked_at = now;
                }
            }
            store.save_worker(worker.clone()).await.map_err(rejection)?;
        }
        if !operations && let Some(pending) = worker.pending_hash.take() {
            let bundle = setup.engine.fetch_worker_bundle(product.to_string(), Some(pending.clone())).await?;
            validate_bundle(&bundle)?;
            if bundle.content_hash != pending { return Err(rejection("cached worker bundle hash mismatch")) }
            worker.content_hash = pending;
            worker.manifest = bundle.manifest;
            store.save_worker(worker.clone()).await.map_err(rejection)?;
        }
        if running.get(product).is_some_and(|running| !running.execution.is_closed() && running.content_hash == worker.content_hash) { return Ok(()) }
        self.stop(running, product).await?;
        let bundle = setup.engine.fetch_worker_bundle(product.to_string(), Some(worker.content_hash.clone())).await?;
        validate_bundle(&bundle)?;
        if bundle.content_hash != worker.content_hash { return Err(rejection("installed worker bundle hash mismatch")) }
        let host = self.host.get().and_then(Weak::upgrade).ok_or_else(|| rejection("worker runtime stopped"))?;
        let product_context = ProductContext::new_with_execution(product.to_string(), ProductExecutionKind::Worker).map_err(rejection)?;
        let execution = host.open_worker_execution(product_context, setup.engine.chat_callbacks(product.to_string()), setup.engine.pocket_callbacks(product.to_string()));
        let weak = self.own.get().expect("worker supervisor attached").clone();
        let weak_execution = Arc::downgrade(&execution);
        let product_id = product.to_string();
        execution.observe_disconnect(Arc::new(move || {
            if let Some(workers) = weak.upgrade() { workers.engine_lost(product_id.clone(), weak_execution.clone(), "worker engine connection closed".to_string()); }
        }));
        if self.suspended.load(Ordering::Acquire) {
            execution.shutdown();
            return Ok(())
        }
        if let Err(error) = setup.engine.start_worker(product.to_string(), execution.clone(), bundle) {
            execution.shutdown();
            return Err(error)
        }
        running.insert(product.to_string(), RunningWorker { execution, content_hash: worker.content_hash });
        Ok(())
    }

    async fn stop(&self, running: &mut HashMap<String, RunningWorker>, product: &str) -> Result<(), HostRejection> {
        if let Some(worker) = running.get(product) {
            worker.execution.shutdown();
            self.setup.get().expect("running worker has an engine").engine.stop_worker(product.to_string()).await?;
            running.remove(product);
        }
        Ok(())
    }

    /// Fence asynchronous engine failures against the execution that actually failed.
    pub fn engine_failed(self: &Arc<Self>, execution: Arc<NativeProductExecution>, reason: String) {
        tracing::error!(product = %execution.product_id(), %reason, "worker engine failed");
        self.engine_lost(execution.product_id(), Arc::downgrade(&execution), reason);
    }

    fn engine_lost(self: &Arc<Self>, product: String, execution: Weak<NativeProductExecution>, reason: String) {
        let workers = self.clone();
        (self.spawner)(Box::pin(async move {
            let result = async {
                if workers.suspended.load(Ordering::Acquire) { return Ok(()) }
                let store = workers.storage.current()?.for_product(&product).map_err(rejection)?;
                let mut running = workers.running.lock().await;
                if !running.get(&product).is_some_and(|current| execution.ptr_eq(&Arc::downgrade(&current.execution))) { return Ok(()) }
                let reason = match workers.stop(&mut running, &product).await {
                    Ok(()) => reason,
                    Err(error) => format!("{reason}; teardown failed: {error}"),
                };
                workers.retry(&store, &product, reason).await?;
                Ok(())
            }.await;
            if let Err(error) = result { workers.report(error); }
        }));
    }

    fn report(&self, error: HostRejection) {
        tracing::error!(reason = %error, "worker lifecycle failed");
    }
}

impl WorkerDemandObserver for NativeWorkers {
    fn worker_demand_changed(&self, product_id: &str, transition: WorkerTransition) {
        let mut demand = self.demand.lock().expect("worker demand mutex poisoned");
        match transition {
            WorkerTransition::Start => { demand.insert(product_id.to_string()); }
            WorkerTransition::Stop => { demand.remove(product_id); }
        }
        drop(demand);
        self.changed(product_id.to_string());
    }
}

fn now_millis() -> Result<i64, HostRejection> {
    let duration = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(rejection)?;
    i64::try_from(duration.as_millis()).map_err(rejection)
}

fn validate_bundle(bundle: &WorkerBundle) -> Result<(), HostRejection> {
    if bundle.content_hash.is_empty() || bundle.local_path.is_empty() || bundle.manifest.is_empty() {
        return Err(rejection("worker bundle must include a content hash, manifest and local directory"))
    }
    Ok(())
}

fn rejection(reason: impl ToString) -> HostRejection {
    HostRejection::Rejected { reason: reason.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::tests::{EventCallbacks, TestWalletSecrets, native_host_runtime_config};

    #[derive(Default)]
    struct Engine {
        current: Mutex<u8>,
        fail_stop: AtomicBool,
        events: Mutex<Vec<(String, Option<Vec<u8>>)>>,
    }

    #[async_trait::async_trait]
    impl NativeWorkerEngineHost for Engine {
        fn chat_callbacks(&self, _: String) -> Option<Arc<dyn NativeChatCallbacks>> { None }
        fn pocket_callbacks(&self, _: String) -> Option<Arc<dyn NativePocketCallbacks>> { None }
        fn start_worker(&self, product: String, _: Arc<NativeProductExecution>, bundle: WorkerBundle) -> Result<(), HostRejection> {
            self.events.lock().unwrap().push((product, Some(bundle.content_hash)));
            Ok(())
        }
        async fn stop_worker(&self, product: String) -> Result<(), HostRejection> {
            self.events.lock().unwrap().push((product, None));
            if self.fail_stop.swap(false, Ordering::AcqRel) { return Err(rejection("engine teardown failed")) }
            Ok(())
        }
        async fn fetch_worker_bundle(&self, _: String, content_hash: Option<Vec<u8>>) -> Result<WorkerBundle, HostRejection> {
            let content_hash = content_hash.unwrap_or_else(|| vec![*self.current.lock().unwrap()]);
            Ok(WorkerBundle { manifest: b"{}".to_vec(), local_path: format!("/bundles/{}", hex::encode(&content_hash)), content_hash })
        }
    }

    fn host(callbacks: Arc<EventCallbacks>, config: super::super::HostRuntimeConfig, engine: Arc<Engine>) -> Arc<NativeTrUApiHostRuntime> {
        let host = NativeTrUApiHostRuntime::with_runtime_config(callbacks, Arc::new(TestWalletSecrets), config).unwrap();
        assert!(host.set_worker_engine_host(engine));
        futures::executor::block_on(host.activate_wallet("fixture-wallet".to_string(), None)).unwrap();
        host
    }

    #[test]
    fn reasons_share_one_engine_and_operations_defer_bundle_updates() {
        let engine = Arc::new(Engine::default());
        let host = host(Arc::new(EventCallbacks::new()), native_host_runtime_config(), engine.clone());
        futures::executor::block_on(async {
            let product = "chat.dot".to_string();
            host.notify_worker_intent(product.clone(), WorkerIntentAction::Add, WorkerModality::Chat).await.unwrap();
            host.notify_worker_intent(product.clone(), WorkerIntentAction::Add, WorkerModality::Card { card_id: "card".into() }).await.unwrap();
            let operation = host.begin_worker_operation(product.clone(), None).await.unwrap();
            *engine.current.lock().unwrap() = 1;
            host.update_workers().await.unwrap();
            assert_eq!(engine.events.lock().unwrap().as_slice(), &[(product.clone(), Some(vec![0]))]);
            assert_eq!(host.workers().await.unwrap()[0].pending_hash, Some(vec![1]));
            assert_eq!(host.worker_operations().await.unwrap(), vec![operation.clone()]);
            host.end_worker_operation(product.clone(), operation.operation_id).await.unwrap();
            host.notify_worker_intent(product.clone(), WorkerIntentAction::Remove, WorkerModality::Chat).await.unwrap();
            host.notify_worker_intent(product.clone(), WorkerIntentAction::Remove, WorkerModality::Card { card_id: "card".into() }).await.unwrap();
            assert_eq!(*engine.events.lock().unwrap(), vec![(product.clone(), Some(vec![0])), (product.clone(), None), (product.clone(), Some(vec![1])), (product, None)]);
            assert!(host.workers().await.unwrap().is_empty());
        });
    }

    #[test]
    fn failed_engine_teardown_is_retried_before_restarting_the_same_bundle() {
        let engine = Arc::new(Engine::default());
        let host = host(Arc::new(EventCallbacks::new()), native_host_runtime_config(), engine.clone());
        futures::executor::block_on(async {
            host.notify_worker_intent("chat.dot".into(), WorkerIntentAction::Add, WorkerModality::Chat).await.unwrap();
            engine.fail_stop.store(true, Ordering::Release);
            assert!(host.suspend_workers().await.is_err());
            host.update_workers().await.unwrap();
            assert_eq!(*engine.events.lock().unwrap(), vec![
                ("chat.dot".into(), Some(vec![0])),
                ("chat.dot".into(), None),
                ("chat.dot".into(), None),
                ("chat.dot".into(), Some(vec![0])),
            ]);
            host.lock_wallet().await.unwrap();
        });
    }

    #[test]
    fn wallet_lock_stops_engines_and_reopen_restores_durable_reasons() {
        let callbacks = Arc::new(EventCallbacks::new());
        let config = native_host_runtime_config();
        let first_engine = Arc::new(Engine::default());
        let first = host(callbacks.clone(), config.clone(), first_engine.clone());
        futures::executor::block_on(first.notify_worker_intent("chat.dot".into(), WorkerIntentAction::Add, WorkerModality::Chat)).unwrap();
        futures::executor::block_on(first.lock_wallet()).unwrap();
        assert_eq!(*first_engine.events.lock().unwrap(), vec![("chat.dot".into(), Some(vec![0])), ("chat.dot".into(), None)]);
        drop(first);
        let restored_engine = Arc::new(Engine::default());
        let restored = host(callbacks, config, restored_engine.clone());
        assert_eq!(*restored_engine.events.lock().unwrap(), vec![("chat.dot".into(), Some(vec![0]))]);
        futures::executor::block_on(restored.lock_wallet()).unwrap();
    }
}
