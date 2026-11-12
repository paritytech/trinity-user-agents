use super::*;
use crate::{
	PairedSsoPeer,
	native::tests::*,
	platform::{
		ExecutableKind, PermissionAuthorizationRequest, PermissionAuthorizationStatus,
		PermissionDecision,
	},
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
	let host = NativeTrUApiHostRuntime::with_runtime_config(callbacks.clone(), config)
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
	let host = NativeTrUApiHostRuntime::with_runtime_config(callbacks.clone(), config)
		.expect("host runtime config should be valid");
	let screen = host
		.open_product_execution(
			callbacks.clone(),
			None,
			None,
			native_execution_config("myapp.dot", ExecutableKind::App),
		)
		.expect("open app execution");
	let worker = host
		.open_product_execution(
			callbacks.clone(),
			None,
			None,
			native_execution_config("myapp.dot", ExecutableKind::Worker),
		)
		.expect("open worker execution");

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
	let host = NativeTrUApiHostRuntime::with_runtime_config(callbacks.clone(), config)
		.expect("host runtime config should be valid");
	let execution = host
		.open_product_execution(
			callbacks.clone(),
			None,
			None,
			native_execution_config("myapp.dot", ExecutableKind::App),
		)
		.expect("open app execution");

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
	)
	.expect("host runtime config should be valid");

	assert!(!host.runtime.set_device_pairing_observer(Arc::new(Inert)));
}

#[test]
fn process_runtime_shares_authority_and_replaces_one_chat_execution_per_product() {
	let host = NativeTrUApiHostRuntime::with_runtime_config(
		Arc::new(EventCallbacks::new()),
		native_host_runtime_config(),
	)
	.expect("host runtime config should be valid");
	let app = host
		.open_product_execution(
			Arc::new(EventCallbacks::new()),
			None,
			None,
			native_execution_config("shared.dot", ExecutableKind::App),
		)
		.expect("App execution should open");
	let chat_host = Arc::new(EventCallbacks::new());
	let chat = host
		.open_product_execution(
			chat_host.clone(),
			Some(chat_host.clone()),
			None,
			native_execution_config("shared.dot", ExecutableKind::Worker),
		)
		.expect("Chat execution should open");

	assert!(Arc::ptr_eq(&app.runtime, &chat.runtime));
	assert!(matches!(
		app.publish_chat_action(text_chat_action("denied")),
		Err(crate::ProductRuntimeError::Denied)
	));
	chat.publish_chat_action(text_chat_action("buffered"))
		.expect("Chat action should buffer before connection");

	let replacement = host
		.open_product_execution(
			chat_host.clone(),
			Some(chat_host.clone()),
			None,
			native_execution_config("shared.dot", ExecutableKind::Worker),
		)
		.expect("replacement Chat execution should open");
	assert!(matches!(
		chat.publish_chat_action(text_chat_action("closed")),
		Err(crate::ProductRuntimeError::Closed)
	));
	assert!(!replacement.closed.load(Ordering::Acquire));
	replacement
		.publish_chat_action(text_chat_action("fresh"))
		.expect("replacement execution has a fresh buffer");
}

#[test]
fn a_renderer_action_reaches_the_product_that_rendered_it() {
	let host = NativeTrUApiHostRuntime::with_runtime_config(
		Arc::new(EventCallbacks::new()),
		native_host_runtime_config(),
	)
	.expect("host runtime config should be valid");
	let execution = host
		.open_product_execution(
			Arc::new(EventCallbacks::new()),
			None,
			None,
			native_execution_config("chat.dot", ExecutableKind::Worker),
		)
		.expect("Worker execution should open");

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

#[test]
fn an_expanded_card_face_request_reaches_only_the_execution_that_made_it() {
	let host = NativeTrUApiHostRuntime::with_runtime_config(
		Arc::new(EventCallbacks::new()),
		native_host_runtime_config(),
	)
	.expect("host runtime config should be valid");
	let open_widget = |product_id: &str| {
		let callbacks = Arc::new(EventCallbacks::new());
		let execution = host
			.open_product_execution(
				callbacks.clone(),
				None,
				None,
				native_execution_config(product_id, ExecutableKind::Widget),
			)
			.expect("Widget execution should open");
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

	assert!(reply.is_ok(), "the card's own host applied the request: {reply:?}");
	assert_eq!(*card_callbacks.face_requests.lock().expect("face requests"), [false]);
	assert!(
		other_callbacks.face_requests.lock().expect("face requests").is_empty(),
		"another product's card must not be asked to move its face"
	);
}

#[test]
fn product_execution_routes_chain_events_to_shared_and_scoped_services() {
	let host = NativeTrUApiHostRuntime::with_runtime_config(
		Arc::new(EventCallbacks::new()),
		native_host_runtime_config(),
	)
	.expect("host runtime config should be valid");
	let execution = host
		.open_product_execution(
			Arc::new(EventCallbacks::new()),
			None,
			None,
			native_execution_config("chain.dot", ExecutableKind::App),
		)
		.expect("App execution should open");
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
	)
	.expect("host runtime config should be valid");
	let execution = host
		.open_product_execution(
			Arc::new(EventCallbacks::new()),
			None,
			None,
			native_execution_config("chain.dot", ExecutableKind::App),
		)
		.expect("App execution should open");

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
	)
	.expect("host runtime config should be valid");
	let execution = host
		.open_product_execution(
			Arc::new(EventCallbacks::new()),
			None,
			None,
			native_execution_config("chain.dot", ExecutableKind::App),
		)
		.expect("App execution should open");

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
	)
	.unwrap();
	let execution = host
		.open_product_execution(
			callbacks.clone(),
			None,
			None,
			native_execution_config("motion.dot", ExecutableKind::App),
		)
		.unwrap();
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
	)
	.unwrap();
	let open = || {
		host.open_product_execution(
			callbacks.clone(),
			None,
			None,
			native_execution_config("fetch.dot", ExecutableKind::App),
		)
		.unwrap()
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
			"INSERT INTO durable_tx (domain, tx_hash, birth_number, birth_hash, period, status)
             VALUES ('nft', zeroblob(32), 0, zeroblob(32), 64, 'PENDING')",
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
	);

	assert!(matches!(result, Err(NativeRuntimeConfigError::DatabaseUnavailable { .. })));
}

/// A host opens an execution only for a product its stack serves, so a name
/// under another network's TLD gets no runtime.
#[test]
fn an_execution_opens_only_for_a_product_the_stack_serves() {
	let callbacks = Arc::new(EventCallbacks::new());
	let mut config = native_host_runtime_config();
	config.local_session_secret = None;
	config.local_session_lite_username = None;
	let host = NativeTrUApiHostRuntime::with_runtime_config(callbacks.clone(), config)
		.expect("host runtime config should be valid");
	let open = |product_id: &str| {
		host.open_product_execution(
			callbacks.clone(),
			None,
			None,
			native_execution_config(product_id, ExecutableKind::App),
		)
		.map(|_| ())
	};

	assert!(open("myapp.dot").is_ok());
	assert!(matches!(open("myapp.elsewhere"), Err(NativeRuntimeConfigError::Invalid { .. })));
}

/// A worker reference for a product the stack does not serve is dropped: the
/// worker could never open, so the host would be asked to start it for good.
#[test]
fn a_worker_reference_for_an_unserved_product_starts_nothing() {
	let callbacks = Arc::new(EventCallbacks::new());
	let host = NativeTrUApiHostRuntime::with_runtime_config(
		callbacks.clone(),
		native_host_runtime_config(),
	)
	.expect("host runtime config should be valid");

	host.acquire_worker(ProductId::for_test("shared.elsewhere"));
	host.acquire_worker(ProductId::for_test("shared.dot"));

	assert_eq!(
		*callbacks.worker_demand.lock().expect("worker demand mutex poisoned"),
		vec![(ProductId::for_test("shared.dot"), crate::WorkerTransition::Start)]
	);
}
