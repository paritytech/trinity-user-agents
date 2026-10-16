//! Runs the example workers in `rust/guests` against an in-process host.

use std::{
	collections::HashMap,
	path::{Path, PathBuf},
	process::Command,
	sync::{Arc, Mutex, OnceLock},
};

use futures::{StreamExt, executor::block_on, stream};
use parity_scale_codec::{Decode, Encode};
use truapi::{
	CallContext, CallError, CancellationToken, HostCallEvent, HostCalls, Subscription, WasmEnv,
	WasmWorker, WasmWorkerError,
	api::{Account, Chat, LocalStorage, Preimage},
	v01,
	versioned::{
		FromLatest, Versioned,
		account::{HostGetUserIdError, HostGetUserIdRequest, HostGetUserIdResponse},
		chat::{
			HostChatCreateRoomError, HostChatCreateRoomRequest, HostChatCreateRoomResponse,
			ProductChatMessagesQueryItem, ProductChatMessagesQueryRequest,
			ProductChatParticipantsQueryItem, ProductChatParticipantsQueryRequest,
			ProductChatQueryError, ProductChatReactionsQueryRequest, ProductChatUserActionError,
			ProductChatUserActionItem, ProductChatUserActionRequest,
		},
		local_storage::{
			HostLocalStorageClearError, HostLocalStorageClearRequest,
			HostLocalStorageClearResponse, HostLocalStorageReadError, HostLocalStorageReadRequest,
			HostLocalStorageReadResponse, HostLocalStorageWriteError, HostLocalStorageWriteRequest,
			HostLocalStorageWriteResponse,
		},
		preimage::{
			RemotePreimageLookupSubscribeError, RemotePreimageLookupSubscribeItem,
			RemotePreimageLookupSubscribeRequest, RemotePreimageSubmitError,
			RemotePreimageSubmitRequest, RemotePreimageSubmitResponse,
		},
		renderer::{ProductRendererRenderItem, ProductRendererRenderRequest},
	},
};

#[derive(Default)]
struct TestHost {
	signed_out: bool,
	preimages: Mutex<Vec<Vec<u8>>>,
	lookup_cancel: Mutex<Option<CancellationToken>>,
	lookup_cancelled_when_user_id_asked: Mutex<Option<bool>>,
	storage: Mutex<HashMap<String, Vec<u8>>>,
	rooms: Mutex<Vec<String>>,
}

#[truapi::async_trait]
impl Chat for TestHost {
	async fn create_room(
		&self,
		_cx: &CallContext,
		request: HostChatCreateRoomRequest,
	) -> Result<HostChatCreateRoomResponse, CallError<HostChatCreateRoomError>> {
		let version = request.version();
		self.rooms
			.lock()
			.unwrap()
			.push(truapi::versioned::IntoLatest::into_latest(request).room_id);
		let created =
			v01::HostChatCreateRoomResponse { status: v01::ChatRoomRegistrationStatus::New };
		Ok(HostChatCreateRoomResponse::from_latest(created, version))
	}
}

#[truapi::async_trait]
impl LocalStorage for TestHost {
	async fn read(
		&self,
		_cx: &CallContext,
		request: HostLocalStorageReadRequest,
	) -> Result<HostLocalStorageReadResponse, CallError<HostLocalStorageReadError>> {
		let HostLocalStorageReadRequest::V1(request) = request else {
			return Err(CallError::Unsupported);
		};
		let value = self.storage.lock().unwrap().get(&request.key).cloned();
		Ok(HostLocalStorageReadResponse::V2(v01::HostLocalStorageReadResponse { value }))
	}

	async fn write(
		&self,
		_cx: &CallContext,
		request: HostLocalStorageWriteRequest,
	) -> Result<HostLocalStorageWriteResponse, CallError<HostLocalStorageWriteError>> {
		let HostLocalStorageWriteRequest::V1(request) = request;
		self.storage.lock().unwrap().insert(request.key, request.value);
		Ok(HostLocalStorageWriteResponse::V1)
	}

	async fn clear(
		&self,
		_cx: &CallContext,
		request: HostLocalStorageClearRequest,
	) -> Result<HostLocalStorageClearResponse, CallError<HostLocalStorageClearError>> {
		let HostLocalStorageClearRequest::V1(request) = request;
		self.storage.lock().unwrap().remove(&request.key);
		Ok(HostLocalStorageClearResponse::V1)
	}
}

#[truapi::async_trait]
impl Account for TestHost {
	async fn get_user_id(
		&self,
		_cx: &CallContext,
		_request: HostGetUserIdRequest,
	) -> Result<HostGetUserIdResponse, CallError<HostGetUserIdError>> {
		let lookup_cancel = self.lookup_cancel.lock().unwrap();
		*self.lookup_cancelled_when_user_id_asked.lock().unwrap() =
			lookup_cancel.as_ref().map(CancellationToken::is_cancelled);
		if self.signed_out {
			return Err(CallError::Domain(HostGetUserIdError::V1(
				v01::HostGetUserIdError::NotConnected,
			)));
		}
		Ok(HostGetUserIdResponse::V1(v01::HostGetUserIdResponse {
			primary_username: "alice.dot".to_string(),
		}))
	}
}

#[truapi::async_trait]
impl Preimage for TestHost {
	async fn submit(
		&self,
		_cx: &CallContext,
		request: RemotePreimageSubmitRequest,
	) -> Result<RemotePreimageSubmitResponse, CallError<RemotePreimageSubmitError>> {
		let RemotePreimageSubmitRequest::V1(value) = request;
		self.preimages.lock().unwrap().push(value);
		Ok(RemotePreimageSubmitResponse::V1(b"key".to_vec()))
	}

	/// Unknown first, then resolved, then open until cancelled.
	async fn lookup_subscribe(
		&self,
		cx: &CallContext,
		_request: RemotePreimageLookupSubscribeRequest,
	) -> Subscription<
		RemotePreimageLookupSubscribeItem,
		CallError<RemotePreimageLookupSubscribeError>,
	> {
		let value = self.preimages.lock().unwrap().last().cloned();
		*self.lookup_cancel.lock().unwrap() = Some(cx.cancel().clone());
		let items = [None, value].map(|value| {
			Ok(RemotePreimageLookupSubscribeItem::V1(v01::RemotePreimageLookupSubscribeItem {
				value,
			}))
		});
		let cancelled = cx.cancel().cancelled();
		Subscription::new(stream::iter(items).chain(stream::pending()).take_until(cancelled))
	}
}

fn guest(name: &str) -> Vec<u8> {
	static BUILT: OnceLock<PathBuf> = OnceLock::new();
	let target = BUILT.get_or_init(|| {
		let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../guests/Cargo.toml");
		let target_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("guests");
		let status = Command::new(env!("CARGO"))
			.args(["build", "--release", "--target", "wasm32-unknown-unknown"])
			.arg("--manifest-path")
			.arg(manifest)
			.arg("--target-dir")
			.arg(&target_dir)
			.status()
			.expect("cargo runs");
		assert!(status.success(), "guest workers failed to build");
		target_dir.join("wasm32-unknown-unknown/release")
	});
	std::fs::read(target.join(format!("{name}_guest.wasm"))).expect("guest worker was built")
}

fn env(host: impl Into<Arc<TestHost>>) -> WasmEnv<TestHost> {
	let mut env = WasmEnv::new(host.into());
	env.link_account();
	env.link_chat();
	env.link_local_storage();
	env.link_preimage();
	env
}

/// Run `worker` on a thread of its own, writing its log lines to `lines`.
fn spawn(
	worker: WasmWorker,
	lines: &Arc<Mutex<Vec<String>>>,
) -> std::thread::JoinHandle<Result<(), WasmWorkerError>> {
	let log = lines.clone();
	let worker = worker.with_log(move |line| log.lock().unwrap().push(line.to_string()));
	std::thread::spawn(move || block_on(worker.run()))
}

/// Every answer of one call of the host, up to its end.
fn ask(calls: &HostCalls, method: &str, request: impl Encode) -> Vec<HostCallEvent> {
	block_on(calls.stream(method, request.encode()).collect())
}

fn item<T: Decode>(event: &HostCallEvent) -> T {
	let HostCallEvent::Item(payload) = event else { panic!("not an item: {event:?}") };
	T::decode(&mut &payload[..]).unwrap()
}

fn end<E: Decode>(event: &HostCallEvent) -> Result<(), CallError<E>> {
	let HostCallEvent::End(payload) = event else { panic!("not an end: {event:?}") };
	Decode::decode(&mut &payload[..]).unwrap()
}

/// The id, the participant and the text of each message of a page.
fn page_of(event: &HostCallEvent) -> Vec<(String, String, String)> {
	let ProductChatMessagesQueryItem::V1(page) = item(event);
	page.messages
		.into_iter()
		.map(|message| {
			let text = match message.body {
				v01::ChatMessageBody::Text { text } => text,
				v01::ChatMessageBody::Custom { message_type, .. } => format!("<{message_type}>"),
				other => format!("{other:?}"),
			};
			(message.message_id, message.origin.participant_id, text)
		})
		.collect()
}

fn notes_page() -> ProductChatMessagesQueryRequest {
	ProductChatMessagesQueryRequest::V1(v01::ProductChatMessagesQueryRequest {
		room_id: "notes".to_string(),
		anchor: v01::ChatPageAnchor::Before { cursor: None },
		page_size: 10,
	})
}

fn new_message(room_id: &str, text: &str) -> ProductChatUserActionRequest {
	ProductChatUserActionRequest::V1(v01::ProductChatUserActionRequest {
		room_id: room_id.to_string(),
		action: v01::ChatUserAction::NewMessage {
			body: v01::ChatMessageBody::Text { text: text.to_string() },
			reply_to: None,
		},
	})
}

fn wait_for_line(lines: &Mutex<Vec<String>>, line: &str) {
	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
	while !lines.lock().unwrap().iter().any(|seen| seen == line) {
		assert!(std::time::Instant::now() < deadline, "no line {line:?} in {lines:?}");
		std::thread::sleep(std::time::Duration::from_millis(10));
	}
}

#[test]
fn a_worker_reaches_its_product_through_typed_calls() {
	let worker = WasmWorker::new(env(TestHost::default()), &guest("hello")).unwrap();

	assert_eq!(block_on(worker.run()).map_err(|error| error.to_string()), Ok(()));
}

#[test]
fn a_domain_error_reaches_the_worker_as_a_typed_error() {
	let host = TestHost { signed_out: true, ..TestHost::default() };
	let worker = WasmWorker::new(env(host), &guest("hello")).unwrap();

	assert_eq!(
		block_on(worker.run()).map_err(|error| error.to_string()),
		Err("worker failed: Domain(NotConnected)".to_string())
	);
}

#[test]
fn a_worker_streams_a_subscription_until_it_has_what_it_waits_for() {
	let host = Arc::new(TestHost::default());
	let worker = WasmWorker::new(env(host.clone()), &guest("preimage")).unwrap();

	let outcome = block_on(worker.run()).map_err(|error| error.to_string());

	let submitted = host.preimages.lock().unwrap().clone();
	assert_eq!((outcome, submitted), (Ok(()), vec![b"hello truapi".to_vec()]));
}

#[test]
fn dropping_a_subscription_in_the_worker_cancels_it_on_the_host_at_once() {
	let host = Arc::new(TestHost::default());
	let worker = WasmWorker::new(env(host.clone()), &guest("early_stop")).unwrap();

	let outcome = block_on(worker.run()).map_err(|error| error.to_string());

	let cancelled_by_next_call = *host.lookup_cancelled_when_user_id_asked.lock().unwrap();
	assert_eq!((outcome, cancelled_by_next_call), (Ok(()), Some(true)));
}

#[test]
fn a_worker_importing_a_method_the_host_does_not_link_is_rejected_before_it_runs() {
	let env = WasmEnv::new(Arc::new(TestHost::default()));

	let error = WasmWorker::new(env, &guest("hello")).err().map(|error| error.to_string());

	assert_eq!(error.as_deref(), Some("unknown import `truapi.account_get_user_id`"));
}

#[test]
fn spawned_tasks_sleep_on_host_timers_and_a_raw_call_round_trips() {
	let host = Arc::new(TestHost::default());
	let lines = Arc::new(Mutex::new(Vec::new()));
	let log = lines.clone();
	let worker = WasmWorker::new(env(host.clone()), &guest("tasks"))
		.unwrap()
		.with_log(move |line| log.lock().unwrap().push(line.to_string()));

	let outcome = block_on(worker.run()).map_err(|error| error.to_string());

	let seed = host.storage.lock().unwrap().get("seed").map(Vec::len);
	let lines = lines.lock().unwrap().clone();
	assert_eq!((outcome, seed, lines), (Ok(()), Some(32), vec!["order: fast, slow".to_string()]));
}

/// With the fuel in the log, each line of the guest starts with the fuel
/// that the worker used so far, and the line itself follows unchanged.
#[test]
fn the_fuel_used_so_far_starts_each_log_line_on_request() {
	let host = Arc::new(TestHost::default());
	let lines = Arc::new(Mutex::new(Vec::new()));
	let log = lines.clone();
	let worker = WasmWorker::new(env(host), &guest("tasks"))
		.unwrap()
		.with_fuel_in_log(true)
		.with_log(move |line| log.lock().unwrap().push(line.to_string()));

	let outcome = block_on(worker.run()).map_err(|error| error.to_string());

	let lines = lines.lock().unwrap().clone();
	let [line] = lines.as_slice() else { panic!("one line: {lines:?}") };
	let (fuel, text) = line
		.strip_prefix("fuel=")
		.and_then(|rest| rest.split_once(' '))
		.unwrap_or_else(|| panic!("no fuel in {line:?}"));
	assert_eq!((outcome, text), (Ok(()), "order: fast, slow"));
	assert!(fuel.parse::<u64>().unwrap() > 0, "{line}");
}

#[test]
fn spawned_tasks_run_on_a_tokio_runtime_too() {
	let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
	let worker = WasmWorker::new(env(TestHost::default()), &guest("tasks")).unwrap();

	let outcome = runtime.block_on(worker.run()).map_err(|error| error.to_string());

	assert_eq!(outcome, Ok(()));
}

#[test]
fn a_worker_that_never_yields_traps_when_its_fuel_ends() {
	let worker = WasmWorker::new(env(TestHost::default()), &guest("busy"))
		.unwrap()
		.with_fuel_per_entry(1_000_000);

	let outcome = block_on(worker.run());

	assert!(matches!(outcome, Err(WasmWorkerError::Trapped { .. })), "{outcome:?}");
}

/// A worker keeps the messages of a room: the host reads them page by page and
/// sends what the user writes, and the worker answers in the version of each
/// request, as a script worker answers over frames.
#[test]
fn the_host_reads_and_writes_a_room_that_the_worker_keeps() {
	let lines = Arc::new(Mutex::new(Vec::new()));
	let worker = WasmWorker::new(env(TestHost::default()), &guest("chat_source")).unwrap();
	let calls = worker.host_calls();
	let running = spawn(worker, &lines);

	let first = ask(&calls, "chat_messages_query", notes_page());
	let sent = ask(&calls, "chat_user_action", new_message("notes", "hello"));
	let second = ask(&calls, "chat_messages_query", notes_page());
	let elsewhere = ask(&calls, "chat_user_action", new_message("elsewhere", "hello"));

	assert_eq!(
		(first.len(), page_of(&first[0]), end::<ProductChatQueryError>(&first[1])),
		(2, vec![("n0".to_string(), "product".to_string(), "<card>".to_string())], Ok(()))
	);
	let ProductChatUserActionItem::V1(v01::ProductChatUserActionItem::MessageCreated(created)) =
		item(&sent[0])
	else {
		panic!("the worker stores the message: {sent:?}");
	};
	assert_eq!(
		(sent.len(), created.message_id, end::<ProductChatUserActionError>(&sent[1])),
		(2, "n1".to_string(), Ok(()))
	);
	assert_eq!(
		page_of(&second[0]),
		[("n0", "product", "<card>"), ("n1", "user", "hello"), ("n2", "product", "noted: hello")]
			.map(|(id, participant, text)| (id.into(), participant.into(), text.into()))
	);
	assert_eq!(
		end::<ProductChatUserActionError>(&elsewhere[0]),
		Err(CallError::Domain(ProductChatUserActionError::V1(
			v01::ProductChatUserActionError::UnknownRoom
		)))
	);
	drop(calls);
	assert!(matches!(running.join().unwrap(), Err(WasmWorkerError::Stalled)));
}

/// A raw handler answers with its own codec, and a method with no handler ends
/// at once with `Unsupported`, so the host never waits on a worker that cannot
/// answer.
#[test]
fn a_raw_handler_answers_and_a_method_with_no_handler_is_unsupported() {
	let lines = Arc::new(Mutex::new(Vec::new()));
	let worker = WasmWorker::new(env(TestHost::default()), &guest("chat_source")).unwrap();
	let calls = worker.host_calls();
	let _running = spawn(worker, &lines);
	let participants =
		ProductChatParticipantsQueryRequest::V1(v01::ProductChatParticipantsQueryRequest {
			room_id: "notes".to_string(),
			query: v01::ChatParticipantsQuery::Count,
		});
	let reactions = ProductChatReactionsQueryRequest::V1(v01::ProductChatReactionsQueryRequest {
		room_id: "notes".to_string(),
		message_id: "n0".to_string(),
	});

	let counted = ask(&calls, "chat_participants_query", participants);
	let unanswered = ask(&calls, "chat_reactions_query", reactions);

	assert_eq!(
		(
			item::<ProductChatParticipantsQueryItem>(&counted[0]),
			end::<ProductChatQueryError>(&counted[1])
		),
		(
			ProductChatParticipantsQueryItem::V1(v01::ProductChatParticipantsQueryItem::Count {
				count: 2
			}),
			Ok(())
		)
	);
	assert_eq!(unanswered.len(), 1);
	assert_eq!(end::<ProductChatQueryError>(&unanswered[0]), Err(CallError::Unsupported));
}

/// A render stays open while the body is on screen. When the host stops it,
/// the worker drops the task that draws it, so a body off screen costs the
/// worker nothing.
#[test]
fn the_host_stops_a_render_and_the_worker_drops_its_task() {
	let lines = Arc::new(Mutex::new(Vec::new()));
	let worker = WasmWorker::new(env(TestHost::default()), &guest("chat_source")).unwrap();
	let calls = worker.host_calls();
	let _running = spawn(worker, &lines);
	let request = ProductRendererRenderRequest::V1(v01::ProductRendererRenderRequest {
		context: v01::RenderContext::ChatMessage {
			room_id: "notes".to_string(),
			message_id: "n0".to_string(),
			message_type: "card".to_string(),
		},
		payload: b"welcome".to_vec(),
	});

	let mut render = calls.stream("renderer_render", request.encode());
	let drawn = block_on(render.next()).expect("the worker draws the body");
	wait_for_line(&lines, "render n0 drawn");
	drop(render);
	wait_for_line(&lines, "render n0 stopped");

	assert_eq!(
		item::<ProductRendererRenderItem>(&drawn),
		ProductRendererRenderItem::V1(v01::RendererNode::String {
			text: "card: welcome".to_string()
		})
	);
}

#[test]
fn a_stopped_worker_writes_what_it_holds_before_it_ends() {
	let host = Arc::new(TestHost::default());
	let worker = WasmWorker::new(env(host.clone()), &guest("stop_flush")).unwrap();
	let stop = worker.stopper();
	let lines = Arc::default();
	let running = spawn(worker, &lines);
	wait_for_line(&lines, "running");
	std::thread::sleep(std::time::Duration::from_millis(50));

	stop.stop();

	let outcome = running.join().unwrap();
	assert!(matches!(outcome, Err(WasmWorkerError::Stopped)), "{outcome:?}");
	let lines = lines.lock().unwrap().clone();
	let flushed: u32 = lines
		.iter()
		.find_map(|line| line.strip_prefix("flushed "))
		.unwrap_or_else(|| panic!("no flush: {lines:?}"))
		.parse()
		.unwrap();
	assert!(flushed > 0, "{lines:?}");
	let stored = host.storage.lock().unwrap().get("count").cloned().unwrap();
	assert_eq!(u32::from_le_bytes(stored.try_into().unwrap()), flushed);
}

#[test]
fn a_stop_ends_the_worker_at_the_end_of_its_grace_time() {
	let host = Arc::new(TestHost::default());
	host.storage.lock().unwrap().insert("hold".to_string(), vec![1]);
	let worker = WasmWorker::new(env(host.clone()), &guest("stop_flush"))
		.unwrap()
		.with_stop_grace(std::time::Duration::from_millis(200));
	let stop = worker.stopper();
	let lines = Arc::default();
	let running = spawn(worker, &lines);
	wait_for_line(&lines, "running");

	let stopped = std::time::Instant::now();
	stop.stop();

	let outcome = running.join().unwrap();
	let elapsed = stopped.elapsed();
	assert!(matches!(outcome, Err(WasmWorkerError::Stopped)), "{outcome:?}");
	assert!(elapsed >= std::time::Duration::from_millis(200), "{elapsed:?}");
	assert!(elapsed < truapi::DEFAULT_STOP_GRACE, "{elapsed:?}");
	let lines = lines.lock().unwrap().clone();
	assert!(lines.contains(&"stopping".to_string()), "{lines:?}");
	assert!(!host.storage.lock().unwrap().contains_key("count"));
}

#[test]
fn a_worker_stopped_before_it_runs_does_not_start() {
	let worker = WasmWorker::new(env(TestHost::default()), &guest("stop_flush")).unwrap();
	let stop = truapi::WorkerStop::new();
	let worker = worker.with_stopper(stop.clone());
	stop.stop();
	let lines = Arc::default();

	let outcome = spawn(worker, &lines).join().unwrap();

	assert!(matches!(outcome, Err(WasmWorkerError::Stopped)), "{outcome:?}");
	assert!(lines.lock().unwrap().is_empty());
}
