//! A product worker compiled to wasm, run in the core behind one native Worker
//! execution.

use std::sync::{
	Arc,
	atomic::{AtomicBool, Ordering},
};

use futures::{FutureExt, channel::oneshot, future::Shared};

use crate::{ProductRuntime, WasmWorker, WasmWorkerError, WorkerStop, subscription::Spawner};

/// The marker of the log lines of a wasm worker in `HostCallbacks::on_core_log`.
pub const WASM_WORKER_LOG_MARKER: &str = "wasm-worker";

/// How a wasm worker ended.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum NativeWasmWorkerEnd {
	/// The entry point of the worker returned without an error.
	Finished,
	/// The host stopped the worker.
	Stopped,
	/// The entry point returned an error, the module trapped, or the worker
	/// waits on no call.
	Failed {
		/// What went wrong, for the log of the host.
		reason: String,
	},
}

/// Observer that a native host gives to receive the end of one wasm worker.
#[uniffi::export(callback_interface)]
pub trait NativeWasmWorkerObserver: Send + Sync {
	/// The worker ended. The core calls this one time for each worker.
	fn on_end(&self, end: NativeWasmWorkerEnd);
}

/// One wasm worker that runs on the executor of the core.
pub struct RunningWasmWorker {
	stop: WorkerStop,
	ended: Arc<AtomicBool>,
	/// Completes when the worker ended.
	done: Shared<oneshot::Receiver<()>>,
}

impl RunningWasmWorker {
	/// Start `worker` on `spawner`. The worker serves its calls through
	/// `product_runtime`, which the core disposes when the worker ends.
	///
	/// When `previous` is a worker of the same execution that still stops, the
	/// new worker starts only after it ended. Thus two workers of one product
	/// never write its product storage at the same time.
	pub fn spawn(
		worker: WasmWorker,
		product_runtime: ProductRuntime,
		observer: Arc<dyn NativeWasmWorkerObserver>,
		spawner: &Spawner,
		previous: Option<RunningWasmWorker>,
	) -> Self {
		let ended = Arc::new(AtomicBool::new(false));
		let task_ended = ended.clone();
		let stop = worker.stopper();
		let (finished, done) = oneshot::channel();
		let previous = previous.map(|previous| previous.done);
		(spawner)(Box::pin(async move {
			if let Some(previous) = previous {
				let _ = previous.await;
			}
			let end = match worker.run().await {
				Ok(()) => NativeWasmWorkerEnd::Finished,
				Err(WasmWorkerError::Stopped) => NativeWasmWorkerEnd::Stopped,
				Err(error) => NativeWasmWorkerEnd::Failed { reason: error.to_string() },
			};
			product_runtime.dispose();
			task_ended.store(true, Ordering::Release);
			let _ = finished.send(());
			observer.on_end(end);
		}));
		Self { stop, ended, done: done.shared() }
	}

	/// Whether the worker still runs. A worker that stops runs until it ends.
	pub fn is_running(&self) -> bool {
		!self.ended.load(Ordering::Acquire)
	}

	/// Whether the host asked the worker to stop.
	pub fn is_stopping(&self) -> bool {
		self.stop.is_requested()
	}

	/// Stop the worker. A worker that takes the stop runs for a short grace
	/// time ([`crate::DEFAULT_STOP_GRACE`]) to write what it holds in memory.
	/// Its observer then receives `Stopped`, unless the worker ended before.
	pub fn stop(&self) {
		self.stop.stop();
	}
}

#[cfg(test)]
mod tests {
	use std::{
		path::{Path, PathBuf},
		process::Command,
		sync::{Mutex, OnceLock},
		time::{Duration, Instant},
	};

	use truapi::v01;

	use super::*;
	use crate::{
		NativeRendererObserver, ProductRuntimeError,
		native::{
			NativeChatQueryAnswer, NativeChatQueryObserver, NativeChatQueryRefusal,
			NativeProductExecution, NativeTrUApiHostRuntime, WsBridgeStartError, tests::*,
		},
		platform::ProductExecutionKind,
	};

	const PRODUCT: &str = "echo.dot";

	/// The example workers of `rust/guests`, built once for every test run.
	fn guest(name: &str) -> Vec<u8> {
		static BUILT: OnceLock<PathBuf> = OnceLock::new();
		let target = BUILT.get_or_init(|| {
			let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
			let target_dir = root.join("../target/wasm-guests");
			let status = Command::new(env!("CARGO"))
				.args(["build", "--release", "--target", "wasm32-unknown-unknown"])
				.arg("--manifest-path")
				.arg(root.join("guests/Cargo.toml"))
				.arg("--target-dir")
				.arg(&target_dir)
				.status()
				.expect("cargo runs");
			assert!(status.success(), "guest workers failed to build");
			target_dir.join("wasm32-unknown-unknown/release")
		});
		std::fs::read(target.join(format!("{name}_guest.wasm"))).expect("guest worker was built")
	}

	#[derive(Default)]
	struct Ends(Mutex<Vec<NativeWasmWorkerEnd>>);

	impl NativeWasmWorkerObserver for Arc<Ends> {
		fn on_end(&self, end: NativeWasmWorkerEnd) {
			self.0.lock().expect("ends mutex poisoned").push(end);
		}
	}

	impl Ends {
		fn all(&self) -> Vec<NativeWasmWorkerEnd> {
			self.0.lock().expect("ends mutex poisoned").clone()
		}
	}

	fn wait_until(what: &str, condition: impl Fn() -> bool) {
		let deadline = Instant::now() + Duration::from_secs(20);
		while !condition() {
			assert!(Instant::now() < deadline, "timed out: {what}");
			std::thread::sleep(Duration::from_millis(20));
		}
	}

	/// A signed-in host and the Worker execution of `echo.dot`, with chat. The user denies every
	/// permission prompt.
	fn worker_execution() -> (Arc<EventCallbacks>, Arc<NativeProductExecution>) {
		execution_with(EventCallbacks::new())
	}

	fn execution_with(
		callbacks: EventCallbacks,
	) -> (Arc<EventCallbacks>, Arc<NativeProductExecution>) {
		let callbacks = Arc::new(callbacks);
		let host = NativeTrUApiHostRuntime::with_runtime_config(
			callbacks.clone(),
			native_host_runtime_config(),
		)
		.expect("host runtime config should be valid");
		let execution = host
			.open_product_execution(
				callbacks.clone(),
				Some(callbacks.clone()),
				None,
				None,
				native_execution_config(PRODUCT, ProductExecutionKind::Worker),
			)
			.expect("open worker execution");
		(callbacks, execution)
	}

	fn start(execution: &NativeProductExecution, name: &str) -> Arc<Ends> {
		let ends = Arc::new(Ends::default());
		execution
			.start_wasm_worker(guest(name), Box::new(ends.clone()))
			.expect("the worker starts");
		ends
	}

	fn worker_log(callbacks: &EventCallbacks) -> Vec<String> {
		callbacks
			.core_logs
			.lock()
			.expect("core logs mutex poisoned")
			.iter()
			.filter(|(marker, _)| marker == WASM_WORKER_LOG_MARKER)
			.map(|(_, detail)| detail.clone())
			.collect()
	}

	fn echo_room_ready(callbacks: &EventCallbacks) -> bool {
		worker_log(callbacks).iter().any(|line| line == "room echo ready")
	}

	/// The worker is served by the chat of its execution: the room it creates and the message it
	/// posts reach the chat callbacks of the host, and what the user posts reaches the worker.
	/// That is the whole path of a chat product on a native host.
	#[test]
	fn a_wasm_worker_chats_through_its_execution() {
		let (callbacks, execution) = worker_execution();
		let ends = start(&execution, "chat_echo");
		wait_until("the worker creates its room", || echo_room_ready(&callbacks));

		execution
			.publish_chat_event(
				"echo".to_string(),
				"native".to_string(),
				v01::ChatEvent::MessagePosted {
					message_id: "m1".to_string(),
					body: v01::ChatMessageBody::Text { text: "hello".to_string() },
					reply_to: None,
				},
			)
			.expect("the user posts");
		wait_until("the worker answers", || {
			!callbacks.chat_posted.lock().expect("posted mutex poisoned").is_empty()
		});

		assert_eq!(
			callbacks.chat_posted.lock().expect("posted mutex poisoned").clone(),
			vec![(
				"echo".to_string(),
				v01::ChatMessageContent::Text { text: "echo: hello".to_string() },
				None
			)]
		);
		assert_eq!(ends.all(), vec![], "a worker that waits on a subscription still runs");
		execution.stop_ws_bridge();
	}

	/// An entry point that returns ends the worker, and the host learns it, so it can count the
	/// worker as gone. The worker asks for the user id through the permissions of its execution.
	#[test]
	fn the_host_learns_that_the_entry_point_returned() {
		let (callbacks, execution) = execution_with(EventCallbacks {
			permission_confirmation_result: crate::platform::PermissionDecision::AllowOnce,
			..EventCallbacks::new()
		});
		let ends = start(&execution, "hello");

		wait_until("the worker ends", || !ends.all().is_empty());

		assert_eq!(ends.all(), vec![NativeWasmWorkerEnd::Finished]);
		assert_eq!(worker_log(&callbacks), vec!["user id: alice".to_string()]);
	}

	/// An error of the entry point reaches the host with its reason, so the host can end the worker
	/// as a fault and say why.
	#[test]
	fn the_host_learns_why_the_worker_failed() {
		let (_callbacks, execution) = worker_execution();
		let ends = start(&execution, "hello");

		wait_until("the worker ends", || !ends.all().is_empty());

		assert_eq!(
			ends.all(),
			vec![NativeWasmWorkerEnd::Failed {
				reason: "worker failed: Domain(PermissionDenied)".to_string()
			}]
		);
	}

	/// A stop of the bridge and a shutdown of the execution both stop the worker, so a host that
	/// stops a worker as it stops a script leaves no module running.
	#[test]
	fn stopping_the_execution_stops_the_worker() {
		let (callbacks, execution) = worker_execution();
		let ends = start(&execution, "chat_echo");
		wait_until("the worker creates its room", || echo_room_ready(&callbacks));

		execution.stop_ws_bridge();
		wait_until("the worker stops", || !ends.all().is_empty());
		assert_eq!(ends.all(), vec![NativeWasmWorkerEnd::Stopped]);

		let again = start(&execution, "chat_echo");
		execution.shutdown();
		wait_until("the worker stops", || !again.all().is_empty());
		assert_eq!(again.all(), vec![NativeWasmWorkerEnd::Stopped]);
	}

	/// A stopped worker gets a grace time in which its calls still reach the host, so that it can
	/// write what it holds in memory (an app that goes to the background stops its workers). A
	/// worker started meanwhile waits until the stopped one ended, so the two never write the
	/// storage of the product at the same time.
	#[test]
	fn a_stopped_worker_writes_its_state_before_the_next_one_starts() {
		let (callbacks, execution) = worker_execution();
		let first = start(&execution, "stop_flush");
		wait_until("the worker runs", || worker_log(&callbacks).contains(&"running".to_string()));

		execution.stop_ws_bridge();
		let second = start(&execution, "stop_flush");
		wait_until("the first worker ends", || !first.all().is_empty());
		wait_until("the second worker runs", || {
			worker_log(&callbacks).iter().filter(|line| *line == "running").count() == 2
		});

		assert_eq!(first.all(), vec![NativeWasmWorkerEnd::Stopped]);
		let log = worker_log(&callbacks);
		assert_eq!(log[0], "running", "{log:?}");
		assert_eq!(log[1], "stopping", "{log:?}");
		assert!(log[2].starts_with("flushed "), "the write after the stop failed: {log:?}");
		assert_eq!(log[3], "running", "{log:?}");
		execution.stop_ws_bridge();
		wait_until("the second worker ends", || !second.all().is_empty());
		assert_eq!(second.all(), vec![NativeWasmWorkerEnd::Stopped]);
	}

	/// One execution serves one worker: a second module or a script bridge beside a running module
	/// would share its chat connection and split the user's actions between them.
	#[test]
	fn an_execution_runs_one_worker_at_a_time() {
		let (callbacks, execution) = worker_execution();
		let _ends = start(&execution, "chat_echo");
		wait_until("the worker creates its room", || echo_room_ready(&callbacks));

		let second =
			execution.start_wasm_worker(guest("chat_echo"), Box::new(Arc::new(Ends::default())));
		assert!(matches!(second, Err(ProductRuntimeError::Rejected { .. })), "{second:?}");
		assert!(matches!(execution.start_ws_bridge(0), Err(WsBridgeStartError::AlreadyRunning)));

		execution.stop_ws_bridge();
		let bridge = execution.start_ws_bridge(0);
		assert!(bridge.is_ok(), "a stopped worker frees the execution: {bridge:?}");
		let third =
			execution.start_wasm_worker(guest("chat_echo"), Box::new(Arc::new(Ends::default())));
		assert!(matches!(third, Err(ProductRuntimeError::Rejected { .. })), "{third:?}");
		execution.stop_ws_bridge();
	}

	/// A module the sandbox cannot load is refused before it runs, and leaves the execution free.
	#[test]
	fn a_bad_module_is_refused_and_leaves_the_execution_free() {
		let (_callbacks, execution) = worker_execution();
		let ends = Arc::new(Ends::default());

		let started = execution.start_wasm_worker(b"not wasm".to_vec(), Box::new(ends.clone()));

		assert!(matches!(started, Err(ProductRuntimeError::Rejected { .. })), "{started:?}");
		assert_eq!(ends.all(), vec![], "a module that never ran has no end");
		let _ends = start(&execution, "hello");
	}

	/// What a host hears of its chat queries and renders, one line each.
	#[derive(Default)]
	struct Heard(Mutex<Vec<String>>);

	impl Heard {
		fn wait_for(&self, count: usize) -> Vec<String> {
			wait_until("the host hears the product", || self.0.lock().unwrap().len() >= count);
			self.0.lock().unwrap().clone()
		}

		fn push(&self, line: String) {
			self.0.lock().unwrap().push(line);
		}
	}

	impl NativeChatQueryObserver for Arc<Heard> {
		fn on_answer(&self, answer: NativeChatQueryAnswer) {
			let line = match answer {
				NativeChatQueryAnswer::Messages { page } => format!(
					"page {}",
					page.messages.iter().map(describe).collect::<Vec<_>>().join(", ")
				),
				NativeChatQueryAnswer::UserAction {
					answer: truapi::latest::ProductChatUserActionItem::MessageCreated(message),
				} => format!("created {}", describe(&message)),
				other => format!("{other:?}"),
			};
			self.push(line);
		}

		fn on_refusal(&self, refusal: NativeChatQueryRefusal) {
			self.push(format!("refused {refusal:?}"));
		}

		fn on_complete(&self) {
			self.push("complete".to_string());
		}
	}

	impl NativeRendererObserver for Arc<Heard> {
		fn on_update(&self, node: v01::RendererNode) {
			self.push(format!("drawn {node:?}"));
		}

		fn on_complete(&self) {
			self.push("render complete".to_string());
		}

		fn on_error(&self, reason: String) {
			self.push(format!("render failed: {reason}"));
		}
	}

	fn describe(message: &v01::ChatMessage) -> String {
		let text = match &message.body {
			v01::ChatMessageBody::Text { text } => text.clone(),
			v01::ChatMessageBody::Custom { message_type, .. } => format!("<{message_type}>"),
			other => format!("{other:?}"),
		};
		format!("{} {}: {text}", message.message_id, message.origin.participant_id)
	}

	fn notes_page() -> v01::ProductChatMessagesQueryRequest {
		v01::ProductChatMessagesQueryRequest {
			room_id: "notes".to_string(),
			anchor: v01::ChatPageAnchor::Before { cursor: None },
			page_size: 20,
		}
	}

	/// A wasm worker keeps the messages of a room as a script worker does: the
	/// host reads a page, sends what the user writes, and draws the custom body
	/// of a message through the entry points of the execution. A render that
	/// the host cancels stops in the worker.
	#[test]
	fn a_wasm_worker_keeps_a_room_and_draws_its_body_for_the_host() {
		let (callbacks, execution) = worker_execution();
		let _ends = start(&execution, "chat_source");
		wait_until("the worker creates its room", || {
			worker_log(&callbacks).iter().any(|line| line == "room notes ready")
		});
		let heard = Arc::new(Heard::default());

		let _first = execution.chat_messages_query(notes_page(), Box::new(heard.clone())).unwrap();
		heard.wait_for(1);
		let action = v01::ProductChatUserActionRequest {
			room_id: "notes".to_string(),
			action: v01::ChatUserAction::NewMessage {
				body: v01::ChatMessageBody::Text { text: "hello".to_string() },
				reply_to: None,
			},
		};
		let _sent = execution.chat_user_action(action, Box::new(heard.clone())).unwrap();
		heard.wait_for(2);
		let _second = execution.chat_messages_query(notes_page(), Box::new(heard.clone())).unwrap();
		heard.wait_for(3);
		let render = execution
			.render(
				v01::ProductRendererRenderRequest {
					context: v01::RenderContext::ChatMessage {
						room_id: "notes".to_string(),
						message_id: "n0".to_string(),
						message_type: "card".to_string(),
					},
					payload: b"welcome".to_vec(),
				},
				Box::new(heard.clone()),
			)
			.unwrap();
		heard.wait_for(4);
		render.cancel();
		drop(render);
		wait_until("the worker stops the render", || {
			worker_log(&callbacks).iter().any(|line| line == "render n0 stopped")
		});

		assert_eq!(
			heard.wait_for(4),
			[
				"page n0 product: <card>",
				"created n1 user: hello",
				"page n0 product: <card>, n1 user: hello, n2 product: noted: hello",
				"drawn String { text: \"card: welcome\" }",
			]
		);
		execution.stop_ws_bridge();
	}

	/// A closed execution has no chat or permissions left to give a worker.
	#[test]
	fn a_closed_execution_refuses_a_worker() {
		let (_callbacks, execution) = worker_execution();
		execution.shutdown();

		let started =
			execution.start_wasm_worker(guest("hello"), Box::new(Arc::new(Ends::default())));

		assert!(matches!(started, Err(ProductRuntimeError::Closed)), "{started:?}");
	}

	#[derive(Default)]
	struct Relay(Mutex<Vec<crate::WakeRelayRegistration>>);

	#[async_trait::async_trait]
	impl crate::native::NativeWakeRelayCallbacks for Relay {
		async fn register_wake_topics(
			&self,
			registration: crate::WakeRelayRegistration,
		) -> Result<(), crate::native::HostRejection> {
			self.0.lock().expect("relay mutex poisoned").push(registration);
			Ok(())
		}
	}

	/// A worker declares its wake watch; the core gives the topic and the key of the device to the
	/// relay of the host, never to the worker; a wake push that the relay signs with that key holds
	/// the worker of the product and starts it. That is the whole path of a push wake on a native
	/// host, up to the push service.
	#[test]
	fn a_wake_push_on_the_watch_of_a_worker_starts_the_worker() {
		let callbacks = Arc::new(EventCallbacks::new());
		let host = NativeTrUApiHostRuntime::with_runtime_config(
			callbacks.clone(),
			native_host_runtime_config(),
		)
		.expect("host runtime config should be valid");
		let relay = Arc::new(Relay::default());
		futures::executor::block_on(host.set_wake_relay(Some(relay.clone())))
			.expect("a relay with no topic yet");
		let execution = host
			.open_product_execution(
				callbacks.clone(),
				Some(callbacks.clone()),
				None,
				None,
				native_execution_config(PRODUCT, ProductExecutionKind::Worker),
			)
			.expect("open worker execution");
		let _ends = start(&execution, "chat_echo");
		wait_until("the worker creates its room", || echo_room_ready(&callbacks));

		execution
			.publish_chat_event(
				"echo".to_string(),
				"native".to_string(),
				v01::ChatEvent::MessagePosted {
					message_id: "m1".to_string(),
					body: v01::ChatMessageBody::Text { text: "/wake ping".to_string() },
					reply_to: None,
				},
			)
			.expect("the user posts");
		wait_until("the worker sets its watch", || {
			!callbacks.chat_posted.lock().expect("posted mutex poisoned").is_empty()
		});
		assert_eq!(
			callbacks.chat_posted.lock().expect("posted mutex poisoned")[0].1,
			v01::ChatMessageContent::Text { text: "echo: /wake ping -> Push".to_string() }
		);
		let registration = relay
			.0
			.lock()
			.expect("relay mutex poisoned")
			.last()
			.cloned()
			.expect("registered");
		let mut topic = [0u8; 32];
		topic[..15].copy_from_slice(b"/statement ping");
		assert_eq!(registration.topics, vec![topic]);

		let push = crate::WakePush::sign(
			&registration.key,
			topic,
			[9; 32],
			crate::unix_time::current_unix_millis(),
		);
		let deliveries =
			futures::executor::block_on(host.handle_wake_push(push.to_json().to_string()))
				.expect("the core takes the push");

		assert!(
			matches!(
				deliveries.as_slice(),
				[crate::PushWakeDelivery {
					product_id,
					room_id,
					watch_id,
					outcome: crate::PushWakeOutcome::Held { .. },
				}] if product_id == PRODUCT && room_id == "echo" && watch_id == "echo"
			),
			"{deliveries:?}"
		);
		assert_eq!(
			callbacks.worker_demand.lock().expect("worker demand mutex poisoned").clone(),
			vec![(PRODUCT.to_string(), crate::WorkerTransition::Start)]
		);
		let forged = crate::WakePush::sign(&[0; 32], topic, [8; 32], push.sent_at_ms);
		assert_eq!(
			futures::executor::block_on(host.handle_wake_push(forged.to_json().to_string())),
			Err(crate::PushWakeRejection::BadSignature)
		);
		execution.stop_ws_bridge();
	}
}
