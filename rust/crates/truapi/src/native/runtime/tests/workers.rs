use super::*;
use crate::native::{
	NativeHostWebViewWorker, NativeWorkerAttempt, NativeWorkerExecutionObserver,
	NativeWorkerExecutionState,
};
use std::{sync::mpsc, time::Duration};

#[derive(Default)]
struct WebViews {
	fail_next_start: AtomicBool,
	starts: Mutex<Vec<Arc<NativeWorkerAttempt>>>,
	stops: Mutex<Vec<u64>>,
}

impl NativeHostWebViewWorker for WebViews {
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
		Some(views.clone()),
	)
	.unwrap();
	(runtime, views, ProductId::for_test("shared.dot"))
}

fn open(attempt: &Arc<NativeWorkerAttempt>) -> Arc<NativeProductExecution> {
	let callbacks = Arc::new(EventCallbacks::new());
	attempt.open_execution(callbacks.clone(), Some(callbacks), None).unwrap()
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
	assert!(runtime.current_worker_execution(product.clone()).is_none());
	assert!(owner.report_worker_module_started());
	assert!(
		runtime.current_worker_execution(product.clone()).is_none(),
		"acknowledgement alone cannot serve a render"
	);
	let mut socket = connect(&owner).await;
	crate::test_support::wait_until(
		|| runtime.current_worker_execution(product.clone()).is_some(),
		"connected worker published",
	);
	let consumer = runtime.current_worker_execution(product.clone()).unwrap();
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
	assert!(runtime.current_worker_execution(product).is_none());
	assert!(matches!(
		consumer.publish_chat_action(text_chat_action("closed")),
		Err(crate::ProductRuntimeError::Closed)
	));
	assert_eq!(
		(views.starts.lock().unwrap().len(), views.stops.lock().unwrap().clone()),
		(1, vec![boot.id()])
	);
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

#[test]
fn a_host_without_worker_support_reports_unavailable_until_the_claim_ends() {
	let callbacks = Arc::new(EventCallbacks::new());
	let runtime = NativeTrUApiHostRuntime::with_runtime_config(
		callbacks.clone(),
		native_host_runtime_config(),
		None,
	)
	.unwrap();
	let product = ProductId::for_test("chat.dot");
	let (send, receive) = mpsc::channel();
	let _subscription = runtime.observe_worker_execution(product.clone(), Arc::new(Observer(send)));
	let next = || receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
	assert!(matches!(next(), NativeWorkerExecutionState::Idle));
	let claim = runtime.claim_worker(product);
	assert!(
		matches!(next(), NativeWorkerExecutionState::Failed { reason } if reason == "no worker host configured")
	);
	let direct = runtime.open_product_execution(
		callbacks,
		None,
		None,
		native_execution_config("chat.dot", ExecutableKind::Worker),
	);
	assert_eq!(
		direct.err(),
		Some(NativeRuntimeConfigError::Invalid {
			reason: "open a worker through its startup attempt".into()
		})
	);
	drop(claim);
	assert!(matches!(next(), NativeWorkerExecutionState::Idle));
}

#[tokio::test]
async fn an_observer_sees_failure_then_the_retried_execution() {
	let (runtime, views, product) = setup();
	let (send, receive) = mpsc::channel();
	let _subscription = runtime.observe_worker_execution(product.clone(), Arc::new(Observer(send)));
	let next = || receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
	assert!(matches!(next(), NativeWorkerExecutionState::Idle));
	views.fail_next_start.store(true, Ordering::Relaxed);
	let _claim = runtime.claim_worker(product.clone());
	assert!(matches!(next(), NativeWorkerExecutionState::Pending));
	assert!(
		matches!(next(), NativeWorkerExecutionState::Failed { reason } if reason == "initial boot error")
	);
	assert!(matches!(next(), NativeWorkerExecutionState::Pending));
	let owner = open(&views.latest());
	let _socket = connect(&owner).await;
	assert!(owner.report_worker_module_started());
	let NativeWorkerExecutionState::Running { execution } = next() else {
		panic!("the retry must publish its ready execution");
	};
	assert!(Arc::ptr_eq(&execution, &runtime.current_worker_execution(product).unwrap()));
}

#[tokio::test]
async fn a_late_webview_cannot_open_publish_or_stop_a_replacement() {
	let (runtime, views, product) = setup();
	let old_claim = runtime.claim_worker(product.clone());
	let old = views.latest();
	let old_owner = open(&old);
	old_claim.release();
	let _new_claim = runtime.claim_worker(product.clone());
	let current = views.latest();
	let current_owner = open(&current);
	let callbacks = Arc::new(EventCallbacks::new());
	assert!(old.open_execution(callbacks, None, None).is_err());
	let _socket = connect(&current_owner).await;
	assert!(current_owner.report_worker_module_started());
	assert!(!old_owner.report_worker_module_started());
	old.failed("late renderer loss".into());
	runtime
		.current_worker_execution(product)
		.unwrap()
		.publish_chat_action(text_chat_action("new"))
		.unwrap();
	assert_eq!(*views.stops.lock().unwrap(), vec![old.id()]);
}

#[tokio::test]
async fn signing_out_revokes_open_workers_and_cancels_pending_starts() {
	let (runtime, views, first) = setup();
	let second = ProductId::for_test("second.dot");
	let _first = runtime.claim_worker(first.clone());
	let boot = views.latest();
	let owner = open(&boot);
	let _socket = connect(&owner).await;
	assert!(owner.report_worker_module_started());
	let consumer = runtime.current_worker_execution(first).unwrap();
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
	assert!(pending.open_execution(Arc::new(EventCallbacks::new()), None, None).is_err());
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
	assert!(owner.report_worker_module_started());
	runtime.reload_worker(product.clone());
	let update = views.latest();
	assert!(runtime.current_worker_execution(product.clone()).is_none());
	assert!(matches!(
		owner.publish_chat_action(text_chat_action("old")),
		Err(crate::ProductRuntimeError::Closed)
	));
	let new_owner = open(&update);
	claim.release();
	assert!(!new_owner.report_worker_module_started());
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
	let owner = attempt.open_execution(callbacks.clone(), None, None).unwrap();
	let endpoint = owner.start_ws_bridge(0).unwrap();
	let url = format!("ws://127.0.0.1:{}/?t={}", endpoint.port, endpoint.token);
	let (send, receive) = mpsc::channel();
	let _observing = runtime.observe_worker_execution(product, Arc::new(Observer(send)));
	let next = || receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
	assert!(matches!(next(), NativeWorkerExecutionState::Pending));
	assert!(owner.report_worker_module_started());
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
