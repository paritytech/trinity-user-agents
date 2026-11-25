use super::*;
use crate::native::{
	NativeWorkerAttempt, NativeWorkerEngine, NativeWorkerExecutionObserver,
	NativeWorkerExecutionState,
};
use std::{sync::mpsc, time::Duration};

#[derive(Default)]
struct WebViews {
	fail_next_start: AtomicBool,
	starts: Mutex<Vec<Arc<NativeWorkerAttempt>>>,
	stops: Mutex<Vec<u64>>,
}

impl NativeWorkerEngine for WebViews {
	fn start(&self, _product_id: ProductId, attempt: Arc<NativeWorkerAttempt>) {
		if self.fail_next_start.swap(false, Ordering::Relaxed) {
			attempt.failed("initial boot error".into());
			assert!(self.stops.lock().unwrap().is_empty(), "the WebView is still starting");
		}
		self.starts.lock().unwrap().push(attempt);
	}

	fn stop(&self, _product_id: ProductId, attempt_id: u64) {
		self.stops.lock().unwrap().push(attempt_id);
	}
}

impl WebViews {
	fn latest(&self) -> Arc<NativeWorkerAttempt> {
		self.starts.lock().unwrap().last().unwrap().clone()
	}
}

fn setup() -> (Arc<NativeTrUApiHostRuntime>, Arc<WebViews>, ProductId) {
	let views = Arc::new(WebViews::default());
	let runtime = NativeTrUApiHostRuntime::with_runtime_config(
		Arc::new(EventCallbacks::new()),
		native_host_runtime_config(),
		views.clone(),
	)
	.unwrap();
	(runtime, views, ProductId::for_test("shared.dot"))
}

fn open(attempt: &Arc<NativeWorkerAttempt>) -> Arc<NativeProductExecution> {
	let callbacks = Arc::new(EventCallbacks::new());
	attempt
		.open_execution(ExecutionCallbacks {
			host: callbacks.clone(),
			chat: Some(callbacks),
			game: None,
		})
		.unwrap()
}

struct RenderObserver;

impl NativeRendererObserver for RenderObserver {
	fn on_update(&self, _node: latest::RendererNode) {}
	fn on_complete(&self) {}
	fn on_error(&self, _reason: String) {}
}

async fn connect(
	owner: &NativeProductExecution,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
	let endpoint = owner.start_ws_bridge(0).unwrap();
	let (socket, _) = tokio_tungstenite::connect_async(format!(
		"ws://127.0.0.1:{}/?t={}",
		endpoint.port, endpoint.token
	))
	.await
	.unwrap();
	crate::test_support::wait_until(
		|| owner.product_control.lock().unwrap().is_some(),
		"authenticated socket installed product control",
	);
	socket
}

#[tokio::test]
async fn a_chat_claim_can_only_release_its_own_share_of_the_pocket_worker() {
	let (runtime, views, product) = setup();
	let chat = runtime.claim_worker(product.clone());
	let pocket = runtime.claim_worker(product.clone());
	let boot = views.latest();
	let owner = open(&boot);
	assert!(running(&runtime, &product).is_none());
	assert!(boot.module_started());
	assert!(running(&runtime, &product).is_none(), "acknowledgement alone cannot serve a render");
	let ledger = runtime.runtime.worker_ledger();
	assert!(!ledger.ran(&product), "a worker that cannot serve yet counts as never having run");
	let mut socket = connect(&owner).await;
	crate::test_support::wait_until(
		|| running(&runtime, &product).is_some(),
		"connected worker published",
	);
	assert!(ledger.ran(&product), "the ledger must hear of a running worker to end a crash run");
	let consumer = running(&runtime, &product).unwrap();
	chat.release();
	chat.release();
	drop(chat);
	assert!(views.stops.lock().unwrap().is_empty());
	use parity_scale_codec::Decode;
	let _render = consumer
		.render(
			latest::ProductRendererRenderRequest {
				context: latest::RenderContext::PocketCard { card_id: "weather".into() },
				payload: vec![],
			},
			Arc::new(RenderObserver),
		)
		.unwrap();
	let start = tokio::time::timeout(Duration::from_secs(5), socket.next())
		.await
		.unwrap()
		.unwrap()
		.unwrap()
		.into_data();
	let request = crate::frame::ProtocolMessage::decode(&mut &start[..]).unwrap();
	assert_eq!(request.payload.message_type, crate::frame::MESSAGE_TYPE_START);
	drop(pocket);
	assert!(running(&runtime, &product).is_none());
	assert!(matches!(
		consumer.publish_chat_action(text_chat_action("closed")),
		Err(crate::ProductRuntimeError::Closed)
	));
	assert_eq!(
		(views.starts.lock().unwrap().len(), views.stops.lock().unwrap().clone()),
		(1, vec![boot.id()])
	);
}

/// The execution an observer registering now is told of, if the worker runs.
fn running(
	runtime: &NativeTrUApiHostRuntime,
	product: &ProductId,
) -> Option<Arc<dyn NativeWorkerExecution>> {
	let (send, receive) = mpsc::channel();
	let _observing = runtime.observe_worker_execution(product.clone(), Arc::new(Observer(send)));
	match receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap() {
		NativeWorkerExecutionState::Running { execution } => Some(execution),
		NativeWorkerExecutionState::Idle | NativeWorkerExecutionState::Pending => None,
	}
}

struct Observer(mpsc::Sender<Option<NativeWorkerExecutionState>>);

impl NativeWorkerExecutionObserver for Observer {
	fn changed(&self, state: NativeWorkerExecutionState) {
		let _ = self.0.send(Some(state));
	}

	fn finished(&self) {
		let _ = self.0.send(None);
	}
}

#[tokio::test]
async fn an_observer_waits_in_pending_while_a_failed_worker_is_retried() {
	let (runtime, views, product) = setup();
	let (send, receive) = mpsc::channel();
	let _subscription = runtime.observe_worker_execution(product.clone(), Arc::new(Observer(send)));
	let next = || receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
	assert!(matches!(next(), NativeWorkerExecutionState::Idle));
	views.fail_next_start.store(true, Ordering::Relaxed);
	let _claim = runtime.claim_worker(product.clone());
	assert!(matches!(next(), NativeWorkerExecutionState::Pending));
	assert_eq!(views.starts.lock().unwrap().len(), 2);
	let retry = views.latest();
	let owner = open(&retry);
	let _socket = connect(&owner).await;
	assert!(retry.module_started());
	let NativeWorkerExecutionState::Running { execution } = next() else {
		panic!("a consumer waiting through the failed start must receive the retry's execution");
	};
	assert!(Arc::ptr_eq(&execution, &running(&runtime, &product).unwrap()));
	retry.failed("renderer lost".into());
	assert!(matches!(next(), NativeWorkerExecutionState::Pending));
}

#[tokio::test]
async fn a_late_webview_cannot_open_publish_or_stop_a_replacement() {
	let (runtime, views, product) = setup();
	let old_claim = runtime.claim_worker(product.clone());
	let old = views.latest();
	let _old_owner = open(&old);
	old_claim.release();
	let _new_claim = runtime.claim_worker(product.clone());
	let current = views.latest();
	assert!(
		old.open_execution(ExecutionCallbacks {
			host: Arc::new(EventCallbacks::new()),
			chat: None,
			game: None,
		})
		.is_err()
	);
	let current_owner = open(&current);
	let _socket = connect(&current_owner).await;
	assert!(!old.module_started());
	assert!(running(&runtime, &product).is_none());
	assert!(current.module_started());
	old.failed("late renderer loss".into());
	running(&runtime, &product)
		.unwrap()
		.publish_chat_action(text_chat_action("new"))
		.unwrap();
	assert_eq!(*views.stops.lock().unwrap(), vec![old.id()]);
}

#[tokio::test]
async fn a_start_that_takes_too_long_is_stopped_and_retried() {
	let (runtime, views, product) = setup();
	let callbacks: Arc<dyn HostCallbacks> = Arc::new(EventCallbacks::new());
	*runtime.workers.lock() =
		Some(crate::native::runtime::workers::Session::new_with_startup_deadline(
			&callbacks,
			views.clone(),
			Duration::from_millis(50),
		));
	let _claim = runtime.claim_worker(product);
	let first = views.latest();
	crate::test_support::wait_until(
		|| views.starts.lock().unwrap().len() == 2,
		"the attempt that never ran was replaced once its deadline passed",
	);
	assert_eq!(views.stops.lock().unwrap().first(), Some(&first.id()));
}

#[tokio::test]
async fn an_attempt_opens_its_execution_once() {
	let (runtime, views, product) = setup();
	let _claim = runtime.claim_worker(product);
	let attempt = views.latest();
	let _owner = open(&attempt);
	assert!(
		attempt
			.open_execution(ExecutionCallbacks {
				host: Arc::new(EventCallbacks::new()),
				chat: None,
				game: None,
			})
			.is_err(),
		"a second execution would replace the first, which nothing would then shut down"
	);
}

#[tokio::test]
async fn signing_out_revokes_open_workers_and_cancels_pending_starts() {
	let (runtime, views, first) = setup();
	let second = ProductId::for_test("second.dot");
	let _first = runtime.claim_worker(first.clone());
	let boot = views.latest();
	let owner = open(&boot);
	let _socket = connect(&owner).await;
	assert!(boot.module_started());
	let consumer = running(&runtime, &first).unwrap();
	let _second = runtime.claim_worker(second.clone());
	let pending = views.latest();
	let (send, changes) = mpsc::channel();
	let _observer = runtime.observe_worker_execution(second.clone(), Arc::new(Observer(send)));
	runtime.shutdown_product_workers();
	runtime.shutdown_product_workers();
	assert!(matches!(
		changes.recv_timeout(Duration::from_secs(5)).unwrap(),
		Some(NativeWorkerExecutionState::Pending)
	));
	assert!(matches!(
		changes.recv_timeout(Duration::from_secs(5)).unwrap(),
		Some(NativeWorkerExecutionState::Idle)
	));
	assert!(changes.recv_timeout(Duration::from_secs(5)).unwrap().is_none());
	let _closed = runtime.claim_worker(second);
	let mut stops = views.stops.lock().unwrap().clone();
	stops.sort_unstable();
	assert_eq!((views.starts.lock().unwrap().len(), stops), (2, vec![boot.id(), pending.id()]));
	assert!(matches!(
		consumer.publish_chat_action(text_chat_action("closed")),
		Err(crate::ProductRuntimeError::Closed)
	));
	assert!(
		pending
			.open_execution(ExecutionCallbacks {
				host: Arc::new(EventCallbacks::new()),
				chat: None,
				game: None,
			})
			.is_err()
	);
}

/// Ends the session from inside its second `start` call.
#[derive(Default)]
struct EndsSessionWhileStarting {
	runtime: OnceLock<std::sync::Weak<NativeTrUApiHostRuntime>>,
	starting: AtomicBool,
	starts: Mutex<Vec<u64>>,
	stops: Mutex<Vec<WebViewStop>>,
}

#[derive(Debug, Clone, PartialEq)]
struct WebViewStop {
	attempt: u64,
	during_start: bool,
	session_unlocked: bool,
}

impl NativeWorkerEngine for EndsSessionWhileStarting {
	fn start(&self, _product_id: ProductId, attempt: Arc<NativeWorkerAttempt>) {
		let mut starts = self.starts.lock().unwrap();
		starts.push(attempt.id());
		if starts.len() == 1 {
			return;
		}
		drop(starts);
		self.starting.store(true, Ordering::Relaxed);
		self.runtime.get().unwrap().upgrade().unwrap().shutdown_product_workers();
		self.starting.store(false, Ordering::Relaxed);
	}

	fn stop(&self, _product_id: ProductId, attempt: u64) {
		let runtime = self.runtime.get().unwrap().upgrade().unwrap();
		self.stops.lock().unwrap().push(WebViewStop {
			attempt,
			during_start: self.starting.load(Ordering::Relaxed),
			session_unlocked: runtime.workers.try_lock().is_some(),
		});
	}
}

#[tokio::test]
async fn ending_a_session_disposes_its_webviews_unlocked_and_never_one_still_starting() {
	let host = Arc::new(EndsSessionWhileStarting::default());
	let runtime = NativeTrUApiHostRuntime::with_runtime_config(
		Arc::new(EventCallbacks::new()),
		native_host_runtime_config(),
		host.clone(),
	)
	.unwrap();
	host.runtime.set(Arc::downgrade(&runtime)).unwrap();
	let _idle = runtime.claim_worker(ProductId::for_test("idle.dot"));
	let _ending = runtime.claim_worker(ProductId::for_test("ending.dot"));
	let starts = host.starts.lock().unwrap().clone();
	assert_eq!(
		host.stops.lock().unwrap().clone(),
		vec![
			WebViewStop { attempt: starts[0], during_start: true, session_unlocked: true },
			WebViewStop { attempt: starts[1], during_start: false, session_unlocked: true },
		],
		"the idle WebView goes before the session end returns, with the host free to call back, \
		 and the one being created only after its start call returned"
	);
}

#[tokio::test]
async fn reloading_replaces_only_a_referenced_product_and_a_final_release_cancels_its_boot() {
	let (runtime, views, product) = setup();
	runtime.reload_worker(product.clone());
	assert!(views.starts.lock().unwrap().is_empty());
	let claim = runtime.claim_worker(product.clone());
	let old = views.latest();
	let owner = open(&old);
	let _socket = connect(&owner).await;
	assert!(old.module_started());
	runtime.reload_worker(product.clone());
	let update = views.latest();
	assert!(running(&runtime, &product).is_none());
	assert!(matches!(
		owner.publish_chat_action(text_chat_action("old")),
		Err(crate::ProductRuntimeError::Closed)
	));
	let _new_owner = open(&update);
	claim.release();
	assert!(!update.module_started());
	old.failed("stale during reload".into());
	runtime.reload_worker(product);
	assert_eq!(
		(views.starts.lock().unwrap().len(), views.stops.lock().unwrap().clone()),
		(2, vec![old.id(), update.id()])
	);
}

#[tokio::test]
async fn losing_the_latest_socket_withdraws_a_worker_until_it_can_receive_calls_again() {
	let (runtime, views, product) = setup();
	let _claim = runtime.claim_worker(product.clone());
	let attempt = views.latest();
	let callbacks = Arc::new(EventCallbacks::new());
	let owner = attempt
		.open_execution(ExecutionCallbacks { host: callbacks.clone(), chat: None, game: None })
		.unwrap();
	let endpoint = owner.start_ws_bridge(0).unwrap();
	let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);
	let (send, receive) = mpsc::channel();
	let _observing = runtime.observe_worker_execution(product, Arc::new(Observer(send)));
	let next = || receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
	assert!(matches!(next(), NativeWorkerExecutionState::Pending));
	assert!(attempt.module_started());
	let connect = || tokio_tungstenite::connect_async(url.as_str());
	let (mut first, _) = connect().await.unwrap();
	assert!(matches!(next(), NativeWorkerExecutionState::Running { .. }));
	let (mut next_socket, _) = connect().await.unwrap();
	assert!(matches!(next(), NativeWorkerExecutionState::Running { .. }));
	first.close(None).await.unwrap();
	crate::test_support::wait_until(
		|| {
			callbacks
				.logs
				.lock()
				.unwrap()
				.iter()
				.any(|(marker, _)| marker == crate::markers::WS_BRIDGE_CONNECTION_CLOSED.target)
		},
		"old socket closed",
	);
	assert!(receive.try_recv().is_err(), "a replaced socket cannot withdraw the live one");
	next_socket.close(None).await.unwrap();
	assert!(matches!(next(), NativeWorkerExecutionState::Pending));
	let (_restored, _) = connect().await.unwrap();
	assert!(matches!(next(), NativeWorkerExecutionState::Running { .. }));
	assert_eq!(views.starts.lock().unwrap().len(), 1);
}
