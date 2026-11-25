mod public_profile;
mod workers;

use super::*;
use crate::{
	PairedSsoPeer,
	native::tests::*,
	platform::{PermissionAuthorizationRequest, PermissionAuthorizationStatus, PermissionDecision},
};
use futures::stream::StreamExt;
use std::sync::{Arc, atomic::Ordering};
use truapi::v01;

/// The runtime owns its log sink, so core warnings reach the callbacks it was built with.
#[test]
fn the_runtime_forwards_core_warnings_to_its_callbacks() {
	let callbacks = Arc::new(EventCallbacks::new());
	let mut config = native_host_runtime_config();
	config.local_session_secret = None;
	config.local_session_lite_username = None;
	let host = NativeTrUApiHostRuntime::with_runtime_config(callbacks.clone(), config, None)
		.expect("host runtime config should be valid");

	crate::logging::testing::forward(
		&host._host_sink,
		markers::TRACING_WARN.target,
		"chain call failed",
	);

	let logs = callbacks.logs.lock().unwrap();
	assert!(
		logs.contains(&(markers::TRACING_WARN.target.to_string(), "chain call failed".to_string())),
		"{logs:?}"
	);
}

#[test]
fn a_worker_write_reaches_a_storage_subscription_in_the_products_other_execution() {
	let callbacks = Arc::new(EventCallbacks::new());
	let mut config = native_host_runtime_config();
	config.local_session_secret = None;
	config.local_session_lite_username = None;
	let (host, _claim, worker) = claimed_worker(config, "myapp.dot", callbacks.clone(), None);
	let screen = host.open_app_execution(
		ProductId::for_test("myapp.dot"),
		ExecutionCallbacks { host: callbacks.clone(), chat: None, game: None },
	);

	let key = "myapp.dot/progress".to_string();
	let mut subscription = screen.platform.subscribe_storage(key.clone());
	assert_eq!(
		futures::executor::block_on(futures::StreamExt::next(&mut subscription)),
		Some(Ok(v01::HostLocalStorageChangeItem { value: None })),
		"a subscription opens on the key's current value"
	);

	futures::executor::block_on(worker.platform.write(key, vec![9])).expect("write");

	assert_eq!(
		futures::FutureExt::now_or_never(futures::StreamExt::next(&mut subscription)),
		Some(Some(Ok(v01::HostLocalStorageChangeItem { value: Some(vec![9]) }))),
		"the screen and the worker share one storage namespace, so a write in one \
         reaches a subscription in the other"
	);
}

#[test]
fn a_host_pushed_storage_change_reaches_the_products_subscription() {
	let callbacks = Arc::new(EventCallbacks::new());
	let mut config = native_host_runtime_config();
	config.local_session_secret = None;
	config.local_session_lite_username = None;
	let host = NativeTrUApiHostRuntime::with_runtime_config(callbacks.clone(), config, None)
		.expect("host runtime config should be valid");
	let execution = host.open_app_execution(
		ProductId::for_test("myapp.dot"),
		ExecutionCallbacks { host: callbacks.clone(), chat: None, game: None },
	);

	let key = "myapp.dot/progress".to_string();
	let mut subscription = execution.platform.subscribe_storage(key.clone());
	assert_eq!(
		futures::executor::block_on(futures::StreamExt::next(&mut subscription)),
		Some(Ok(v01::HostLocalStorageChangeItem { value: None })),
		"a subscription opens on the key's current value"
	);

	execution.notify_storage_changed(key, Some(vec![7]));

	assert_eq!(
		futures::FutureExt::now_or_never(futures::StreamExt::next(&mut subscription)),
		Some(Some(Ok(v01::HostLocalStorageChangeItem { value: Some(vec![7]) }))),
		"a change the host made itself still reaches the product"
	);
}

/// The runtime installs its own observer, so a host cannot be left with a
/// pairing nobody forwards.
#[test]
fn the_native_runtime_installs_the_pairing_observer() {
	struct Inert;
	impl crate::DevicePairingObserver for Inert {
		fn device_paired(&self, _device: PairedSsoPeer) {}
	}

	let host = NativeTrUApiHostRuntime::with_runtime_config(
		Arc::new(EventCallbacks::new()),
		native_host_runtime_config(),
		None,
	)
	.expect("host runtime config should be valid");

	assert!(!host.runtime.set_device_pairing_observer(Arc::new(Inert)));
}

#[test]
fn a_renderer_action_reaches_the_product_that_rendered_it() {
	let (_host, _claim, execution) = claimed_worker(
		native_host_runtime_config(),
		"chat.dot",
		Arc::new(EventCallbacks::new()),
		None,
	);

	let admin = execution.admin();
	let mut actions = futures::executor::block_on(truapi::api::Renderer::action_subscribe(
		admin.product_runtime().as_ref(),
		&truapi::CallContext::with_request_id("renderer-1".to_string()),
		truapi::versioned::renderer::HostRendererActionSubscribeRequest::V1,
	));

	let published = truapi::latest::HostRendererActionSubscribeItem {
		context: truapi::latest::RenderContext::ChatMessage {
			room_id: "support".to_string(),
			message_id: "message-1".to_string(),
			message_type: "vote".to_string(),
		},
		action_id: "approve".to_string(),
		payload: Vec::new(),
	};
	execution
		.publish_renderer_action(published.clone())
		.expect("a Worker execution may publish renderer actions");

	let mut cx = core::task::Context::from_waker(futures::task::noop_waker_ref());
	let delivered = match actions.poll_next_unpin(&mut cx) {
		core::task::Poll::Ready(Some(item)) => item,
		other => panic!("a published renderer action must be ready, got {other:?}"),
	};
	let delivered = delivered.expect("expected a renderer action item");
	assert_eq!(truapi::versioned::IntoLatest::into_latest(delivered), published);
	// The scanner checks taps on a connection, while hosts publish them here.
	assert!(admin.product_runtime().connection().recently_tapped());
}

struct Updates(std::sync::Mutex<Vec<crate::ResolvedMaterials>>);

impl crate::NativeMaterialsObserver for Updates {
	fn on_resolved(&self, materials: crate::ResolvedMaterials) {
		self.0.lock().unwrap().push(materials);
	}
}

fn materials_worker()
-> (Arc<NativeTrUApiHostRuntime>, Arc<NativeWorkerClaim>, Arc<NativeProductExecution>) {
	let mut config = native_host_runtime_config();
	config.materials_enabled = true;
	claimed_worker(config, "cards.dot", Arc::new(EventCallbacks::new()), None)
}

/// A material node that falls back in a body a product rendered is the
/// product's to hear about: the resolution on its execution reports it through the
/// product's failure stream. A report is not one of the user's taps.
#[test]
fn a_material_node_that_falls_back_reaches_the_product_that_rendered_it() {
	let (_host, _claim, execution) = materials_worker();
	let admin = execution.admin();
	let mut failures = futures::executor::block_on(truapi::api::Renderer::failure_subscribe(
		admin.product_runtime().as_ref(),
		&truapi::CallContext::with_request_id("failures-1".to_string()),
		truapi::versioned::renderer::HostRendererFailureSubscribeRequest::V1,
	));
	let context = truapi::latest::RenderContext::PocketCard { card_id: "stamps".to_string() };
	let over_the_layer_limit = (0..17)
		.map(|_| truapi::latest::MaterialInstance { material: [7; 32], args: Vec::new() })
		.collect();
	let tree = truapi::latest::RendererNode::Materials {
		props: truapi::latest::MaterialsProps { layers: over_the_layer_limit },
		children: Vec::new(),
	};

	let updates = Box::new(Updates(std::sync::Mutex::new(Vec::new())));
	let worker: Arc<dyn NativeWorkerExecution> = execution.clone();
	let _resolution = worker.resolve_materials(tree, context.clone(), true, updates);
	let delivered = crate::test_support::next(&mut failures)
		.expect("a failure report")
		.expect("a failure item")
		.into_latest();

	assert_eq!(
		delivered,
		truapi::latest::HostRendererFailureSubscribeItem {
			context,
			node: 0,
			material: None,
			reason: truapi::latest::MaterialFailure::LimitExceeded,
		}
	);
	assert!(!admin.product_runtime().connection().recently_tapped());
}

/// A shut execution's products hear of no more failures.
#[test]
fn shutting_an_execution_down_closes_its_failure_stream() {
	let (_host, _claim, execution) = materials_worker();
	let admin = execution.admin();

	execution.shutdown();
	let mut failures = futures::executor::block_on(truapi::api::Renderer::failure_subscribe(
		admin.product_runtime().as_ref(),
		&truapi::CallContext::with_request_id("failures-2".to_string()),
		truapi::versioned::renderer::HostRendererFailureSubscribeRequest::V1,
	));

	assert!(matches!(crate::test_support::next(&mut failures), Some(Err(_))));
}

#[test]
fn an_expanded_card_request_reaches_only_the_execution_that_made_it() {
	let host = NativeTrUApiHostRuntime::with_runtime_config(
		Arc::new(EventCallbacks::new()),
		native_host_runtime_config(),
		None,
	)
	.expect("host runtime config should be valid");
	let open_widget = |product_id: &str| {
		let callbacks = Arc::new(EventCallbacks::new());
		let execution = host.open_widget_execution(
			ProductId::for_test(product_id),
			ExecutionCallbacks { host: callbacks.clone(), chat: None, game: None },
		);
		(execution, callbacks)
	};
	let (card, card_callbacks) = open_widget("card.dot");
	let (_other, other_callbacks) = open_widget("other.dot");

	let admin = card.admin();
	let reply = futures::executor::block_on(truapi::api::ExpandedCard::set_face_shown(
		admin.product_runtime().as_ref(),
		&truapi::CallContext::default(),
		v01::HostExpandedCardSetFaceShownRequest { shown: false }.into(),
	));

	let turned = futures::executor::block_on(truapi::api::ExpandedCard::set_back_shown(
		admin.product_runtime().as_ref(),
		&truapi::CallContext::default(),
		v01::HostExpandedCardSetBackShownRequest { shown: true }.into(),
	));

	assert!(
		reply.is_ok() && turned.is_ok(),
		"the card's own host applied both: {reply:?} {turned:?}"
	);
	assert_eq!(
		(card_callbacks.face_requests(), card_callbacks.back_requests()),
		(vec![false], vec![true])
	);
	assert!(
		other_callbacks.face_requests().is_empty() && other_callbacks.back_requests().is_empty(),
		"another product's card must not be asked to move its face or turn"
	);
}

#[test]
fn product_execution_routes_chain_events_to_shared_and_scoped_services() {
	let host = NativeTrUApiHostRuntime::with_runtime_config(
		Arc::new(EventCallbacks::new()),
		native_host_runtime_config(),
		None,
	)
	.expect("host runtime config should be valid");
	let execution = host.open_app_execution(
		ProductId::for_test("chain.dot"),
		ExecutionCallbacks { host: Arc::new(EventCallbacks::new()), chat: None, game: None },
	);
	let mut shared_responses = host.events.register_chain(41, 0).unwrap();
	let mut scoped_responses = execution.events.register_chain(41, 0).unwrap();
	let response = r#"{"jsonrpc":"2.0","id":"truapi:1","result":true}"#.to_string();

	execution.notify_chain_response(41, response.clone());

	assert_eq!(futures::executor::block_on(shared_responses.next()), Some(response.clone()));
	assert_eq!(futures::executor::block_on(scoped_responses.next()), Some(response));

	execution.notify_chain_closed(41);
	assert_eq!(futures::executor::block_on(shared_responses.next()), None);
	assert_eq!(futures::executor::block_on(scoped_responses.next()), None);
}

/// An execution's close reaches both buses, so it necessarily misses one of
/// them. That miss must not read as a close racing a connect, or every
/// close an execution makes invalidates the shared bus's in-flight
/// connects and they fail with "chain connection closed during setup".
#[test]
fn an_execution_close_does_not_invalidate_shared_bus_connects() {
	let host = NativeTrUApiHostRuntime::with_runtime_config(
		Arc::new(EventCallbacks::new()),
		native_host_runtime_config(),
		None,
	)
	.expect("host runtime config should be valid");
	let execution = host.open_app_execution(
		ProductId::for_test("chain.dot"),
		ExecutionCallbacks { host: Arc::new(EventCallbacks::new()), chat: None, game: None },
	);

	// A connect on the shared bus reads the generation, then an unrelated
	// execution connection closes while it is still in flight.
	let generation = host.events.chain_close_generation();
	let _scoped = execution.events.register_chain(7, 0).expect("scoped registration");
	execution.notify_chain_closed(7);

	assert_eq!(
		host.events.chain_close_generation(),
		generation,
		"an execution's close bumped the shared bus generation",
	);
	assert!(
		host.events.register_chain(99, generation).is_some(),
		"a shared-bus connect was invalidated by an unrelated execution close",
	);
}

/// A close for an id neither bus registered is the real early-close case
/// and must still invalidate the execution's own in-flight connects.
#[test]
fn an_unknown_close_still_invalidates_the_executions_own_connects() {
	let host = NativeTrUApiHostRuntime::with_runtime_config(
		Arc::new(EventCallbacks::new()),
		native_host_runtime_config(),
		None,
	)
	.expect("host runtime config should be valid");
	let execution = host.open_app_execution(
		ProductId::for_test("chain.dot"),
		ExecutionCallbacks { host: Arc::new(EventCallbacks::new()), chat: None, game: None },
	);

	let generation = execution.events.chain_close_generation();
	execution.notify_chain_closed(404);

	assert!(
		execution.events.register_chain(404, generation).is_none(),
		"a close that beat its own connect was not reported as an early close",
	);
}

#[test]
fn native_execution_authorizes_device_permission_from_stored_decision() {
	let callbacks = Arc::new(EventCallbacks::new());
	let host = NativeTrUApiHostRuntime::with_runtime_config(
		callbacks.clone(),
		native_host_runtime_config(),
		None,
	)
	.unwrap();
	let execution = host.open_app_execution(
		ProductId::for_test("motion.dot"),
		ExecutionCallbacks { host: callbacks.clone(), chat: None, game: None },
	);
	let motion = truapi::latest::HostDevicePermissionRequest::Motion;
	let permission = PermissionAuthorizationRequest::Device(motion);
	let authorize = || futures::executor::block_on(execution.authorize_device_permission(motion));
	let prompts = || callbacks.device_permission_calls.load(Ordering::SeqCst);
	let undecided = (authorize().unwrap(), prompts());
	execution
		.set_permission_authorization_status(
			permission.clone(),
			PermissionAuthorizationStatus::Authorized,
		)
		.unwrap();
	let saved = (authorize().unwrap(), prompts());
	execution
		.set_permission_authorization_status(permission, PermissionAuthorizationStatus::Denied)
		.unwrap();
	let denied = (authorize().unwrap(), prompts());
	execution.shutdown();
	let closed = matches!(authorize(), Err(HostRejection::Rejected { .. }));
	assert_eq!((undecided, saved, denied, closed), ((false, 1), (true, 1), (false, 1), true));
}

#[test]
fn native_execution_shares_one_use_permissions_across_connections_only() {
	let callbacks = Arc::new(EventCallbacks {
		remote_permission_result: Ok(PermissionDecision::AllowOnce),
		..EventCallbacks::new()
	});
	let host = NativeTrUApiHostRuntime::with_runtime_config(
		callbacks.clone(),
		native_host_runtime_config(),
		None,
	)
	.unwrap();
	let open = || {
		host.open_app_execution(
			ProductId::for_test("fetch.dot"),
			ExecutionCallbacks { host: callbacks.clone(), chat: None, game: None },
		)
	};
	let execution = open();
	let other = open();
	futures::executor::block_on(async {
		let admin = execution.admin();
		let request = truapi::latest::RemotePermissionRequest {
			permission: truapi::latest::RemotePermission::Remote {
				domains: vec!["api.example.com".to_string()],
			},
		};
		let context = truapi::CallContext::default();
		let sdk_request = || {
			admin
				.product_runtime()
				.request_remote_permission(&context, request.clone().into())
		};
		let granted = sdk_request().await.unwrap().into_latest();
		let permission = PermissionAuthorizationRequest::Remote(request.clone());
		let other_status = other.permission_authorization_status(permission.clone()).await.unwrap();
		let consumed = execution.authorize_remote_permission(request.clone()).await.unwrap();
		let after_use =
			execution.permission_authorization_status(permission.clone()).await.unwrap();
		let prompts_after_use = callbacks.remote_permission_calls.load(Ordering::SeqCst);
		let next_operation = execution.authorize_remote_permission(request.clone()).await.unwrap();
		let other_operation = other.authorize_remote_permission(request.clone()).await.unwrap();
		let prompts_after_operations = callbacks.remote_permission_calls.load(Ordering::SeqCst);
		sdk_request().await.unwrap();
		execution.shutdown();
		let after_shutdown = admin.permission_authorization_status(permission).await.unwrap();
		assert_eq!(
			(
				granted,
				other_status,
				consumed,
				after_use,
				prompts_after_use,
				next_operation,
				other_operation,
				prompts_after_operations,
				after_shutdown
			),
			(
				truapi::latest::RemotePermissionResponse { granted: true },
				PermissionAuthorizationStatus::NotDetermined,
				true,
				PermissionAuthorizationStatus::NotDetermined,
				1,
				true,
				true,
				3,
				PermissionAuthorizationStatus::NotDetermined,
			)
		);
	});
}

#[test]
fn the_runtime_opens_the_core_database_at_startup() {
	// Durable work resumes as soon as the runtime exists, so the database
	// has to be open before the first call reaches it.
	let dir = tempfile::tempdir().unwrap();
	let host = NativeTrUApiHostRuntime::with_runtime_config(
		Arc::new(EventCallbacks::new()),
		HostRuntimeConfig {
			database_directory: dir.path().to_string_lossy().into_owned(),
			..native_host_runtime_config()
		},
		None,
	)
	.expect("host runtime config should be valid");

	let status = futures::executor::block_on(host.core_database_status())
		.expect("the core database is open");

	let file = dir.path().canonicalize().unwrap().join(crate::store::CORE_DB_FILE);
	assert_eq!(
		status.path,
		Some(file.to_string_lossy().into_owned()),
		"the database lives in the configured directory"
	);
}

/// Rows a previous process left live are decided by recovery, so the
/// host has to be asked for its background task as soon as the runtime
/// exists.
#[test]
fn durable_work_left_by_a_previous_process_is_reported_at_startup() {
	let dir = tempfile::tempdir().unwrap();
	let previous = futures::executor::block_on(Db::open(core_db_config(dir.path()))).unwrap();
	futures::executor::block_on(previous.write(|tx| {
		tx.execute(
			"INSERT INTO durable_tx (domain, tx_hash, birth_number, period, status)
             VALUES ('nft', zeroblob(32), 0, 64, 'PENDING')",
			[],
		)?;
		Ok(())
	}))
	.unwrap();
	futures::executor::block_on(previous.close()).unwrap();
	let callbacks = Arc::new(EventCallbacks::new());

	let _host = NativeTrUApiHostRuntime::with_runtime_config(
		callbacks.clone(),
		HostRuntimeConfig {
			database_directory: dir.path().to_string_lossy().into_owned(),
			..native_host_runtime_config()
		},
		None,
	)
	.expect("host runtime config should be valid");

	crate::test_support::wait_until(
		|| *callbacks.durable_work.lock().unwrap() == vec![true],
		"the host is told durable work is pending",
	);
}

#[test]
fn durable_recovery_returns_once_nothing_is_live() {
	let host = NativeTrUApiHostRuntime::with_runtime_config(
		Arc::new(EventCallbacks::new()),
		native_host_runtime_config(),
		None,
	)
	.expect("host runtime config should be valid");

	assert!(futures::executor::block_on(host.run_durable_recovery()).is_ok());
}

#[test]
fn a_missing_database_directory_fails_runtime_creation() {
	// A wrong directory must stop the host at startup, not surface later
	// as durable work that cannot be recorded.
	let dir = tempfile::tempdir().unwrap();
	let missing = dir.path().join("absent");

	let result = NativeTrUApiHostRuntime::with_runtime_config(
		Arc::new(EventCallbacks::new()),
		HostRuntimeConfig {
			database_directory: missing.to_string_lossy().into_owned(),
			..native_host_runtime_config()
		},
		None,
	);

	assert!(matches!(result, Err(NativeRuntimeConfigError::DatabaseUnavailable { .. })));
}

/// The sink a runtime installs carries events as well as lines, so a host that
/// implements the structured callback is handed what the core emits under a
/// marker rather than a line it would have to parse.
#[test]
fn the_runtime_sink_forwards_an_event_to_the_host() {
	use crate::logging::{CoreAttribute, CoreAttributeValue, HostSink as _};

	let callbacks = Arc::new(EventCallbacks::new());
	let held: Arc<dyn HostCallbacks> = callbacks.clone();
	let sink = CallbackSink { callbacks: Arc::downgrade(&held) };
	let attributes = vec![CoreAttribute {
		key: "outcome".to_string(),
		value: CoreAttributeValue::Text { value: "sent".to_string() },
	}];

	sink.event(crate::markers::CORE_STARTED.target, attributes.clone());

	assert_eq!(
		*callbacks.events.lock().expect("events mutex poisoned"),
		vec![(crate::markers::CORE_STARTED.target.to_string(), attributes)],
	);
}

/// A host being torn down must not be resurrected by traffic already in
/// flight, so the sink holds its callbacks weakly on the event path too.
#[test]
fn the_runtime_sink_drops_an_event_for_a_host_that_has_gone() {
	use crate::logging::HostSink as _;

	let callbacks = Arc::new(EventCallbacks::new());
	let held: Arc<dyn HostCallbacks> = callbacks.clone();
	let sink = CallbackSink { callbacks: Arc::downgrade(&held) };
	let watch = Arc::downgrade(&held);
	drop(held);
	drop(callbacks);
	assert!(watch.upgrade().is_none(), "the host outlived both of the test's handles");

	sink.event(crate::markers::CORE_STARTED.target, Vec::new());

	assert!(watch.upgrade().is_none(), "the sink resurrected a host that had gone");
}
