use super::*;
use crate::native::{
	NativeWebViewWorkerHost, NativeWorkerAttempt, NativeWorkerExecutionObserver,
	NativeWorkerExecutionState,
};
use std::{sync::mpsc, time::Duration};

#[derive(Default)]
struct WebViews {
	starts: Mutex<Vec<Arc<NativeWorkerAttempt>>>,
	stops: Mutex<Vec<u64>>,
}

impl NativeWebViewWorkerHost for WebViews {
	fn start(&self, _product_id: ProductId, attempt: Arc<NativeWorkerAttempt>) {
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
	let runtime = NativeTrUApiHostRuntime::with_managed_product_workers(
		Arc::new(EventCallbacks::new()),
		native_host_runtime_config(),
	)
	.unwrap();
	let views = Arc::new(WebViews::default());
	assert!(runtime.enable_product_workers(views.clone()));
	(runtime, views, ProductId::for_test("shared.dot"))
}

fn open(attempt: &Arc<NativeWorkerAttempt>) -> Arc<NativeProductExecution> {
	let callbacks = Arc::new(EventCallbacks::new());
	attempt.open_execution(callbacks.clone(), Some(callbacks), None).unwrap()
}

struct Drawn(mpsc::Sender<latest::RendererNode>);

impl NativeRendererObserver for Drawn {
	fn on_update(&self, node: latest::RendererNode) {
		self.0.send(node).unwrap();
	}
	fn on_complete(&self) {}
	fn on_error(&self, reason: String) {
		panic!("{reason}");
	}
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
	assert!(boot.ready(owner.clone()));
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
	use futures::SinkExt;
	use parity_scale_codec::Decode;
	use tokio_tungstenite::tungstenite::Message;
	let (send, drawn) = mpsc::channel();
	let _render = consumer
		.render(
			latest::ProductRendererRenderRequest {
				context: latest::RenderContext::PocketCard { card_id: "weather".into() },
				payload: vec![],
			},
			Arc::new(Drawn(send)),
		)
		.unwrap();
	let start = tokio::time::timeout(Duration::from_secs(5), socket.next())
		.await
		.unwrap()
		.unwrap()
		.unwrap()
		.into_data();
	let mut reply = crate::frame::ProtocolMessage::decode(&mut &start[..]).unwrap();
	assert_eq!(reply.payload.message_type, crate::frame::MESSAGE_TYPE_START);
	reply.payload.message_type = crate::frame::MESSAGE_TYPE_RECEIVE;
	reply.payload.value =
		truapi::versioned::renderer::ProductRendererRenderItem::V1(latest::RendererNode::Nil)
			.encode();
	socket.send(Message::Binary(reply.encode())).await.unwrap();
	assert_eq!(drawn.recv_timeout(Duration::from_secs(5)).unwrap(), latest::RendererNode::Nil);
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

#[tokio::test]
async fn an_observer_joining_a_retry_sees_pending_then_the_new_execution() {
	let (runtime, views, product) = setup();
	let _claim = runtime.claim_worker(product.clone());
	views.latest().failed("initial boot error".into());
	let retry = views.latest();
	let (send, receive) = mpsc::channel();
	let _subscription = runtime.observe_worker_execution(product.clone(), Arc::new(Observer(send)));
	let next = || receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
	assert!(matches!(next(), NativeWorkerExecutionState::Pending));
	let owner = open(&retry);
	let _socket = connect(&owner).await;
	assert!(retry.ready(owner));
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
	assert!(!old.ready(current_owner.clone()));
	assert!(!current.ready(old_owner.clone()));
	let _socket = connect(&current_owner).await;
	assert!(current.ready(current_owner));
	assert!(!old.ready(old_owner));
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
	assert!(boot.ready(owner));
	let consumer = runtime.current_worker_execution(first).unwrap();
	let _second = runtime.claim_worker(second.clone());
	let pending = views.latest();
	runtime.shutdown_product_workers();
	runtime.shutdown_product_workers();
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

#[test]
fn installing_the_webview_host_drains_existing_core_demand_without_an_external_start() {
	let callbacks = Arc::new(EventCallbacks::new());
	let runtime = NativeTrUApiHostRuntime::with_managed_product_workers(
		callbacks.clone(),
		native_host_runtime_config(),
	)
	.unwrap();
	let product = ProductId::for_test("early.dot");
	runtime.runtime.worker_ledger().acquire(&product);
	let views = Arc::new(WebViews::default());
	assert!(runtime.enable_product_workers(views.clone()));
	assert!(!runtime.enable_product_workers(Arc::new(WebViews::default())));
	let extra = runtime.claim_worker(product.clone());
	extra.release();
	assert!(views.stops.lock().unwrap().is_empty());
	runtime.runtime.worker_ledger().release(&product);
	assert_eq!(
		(
			views.starts.lock().unwrap().len(),
			views.stops.lock().unwrap().len(),
			callbacks.worker_demand.lock().unwrap().len()
		),
		(1, 1, 0)
	);
}

#[test]
fn shutdown_ends_observations_even_if_the_adapter_was_never_installed() {
	let host = NativeTrUApiHostRuntime::with_managed_product_workers(
		Arc::new(EventCallbacks::new()),
		native_host_runtime_config(),
	)
	.unwrap();
	let product = ProductId::for_test("idle.dot");
	let observe = || {
		let (send, receive) = mpsc::channel();
		let subscription = host.observe_worker_execution(product.clone(), Arc::new(Observer(send)));
		(subscription, receive)
	};
	let (_before, before) = observe();
	host.shutdown_product_workers();
	let (_after, after) = observe();
	for observer in [before, after] {
		assert!(matches!(
			observer.recv_timeout(Duration::from_secs(5)).unwrap(),
			Some(NativeWorkerExecutionState::Idle)
		));
		assert!(observer.recv_timeout(Duration::from_secs(5)).unwrap().is_none());
	}
	assert!(!host.enable_product_workers(Arc::new(WebViews::default())));
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
	assert!(old.ready(owner.clone()));
	runtime.reload_worker(product.clone());
	let update = views.latest();
	assert!(runtime.current_worker_execution(product.clone()).is_none());
	assert!(matches!(
		owner.publish_chat_action(text_chat_action("old")),
		Err(crate::ProductRuntimeError::Closed)
	));
	let new_owner = open(&update);
	claim.release();
	assert!(!update.ready(new_owner));
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
	assert!(attempt.ready(owner));
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
