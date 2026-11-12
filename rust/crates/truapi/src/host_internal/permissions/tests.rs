use super::*;
use futures::lock::Mutex;
use std::{
	collections::HashMap,
	sync::{
		LazyLock,
		atomic::{AtomicUsize, Ordering},
	},
};
use strum::VariantArray;
use truapi::{latest::RemotePermission, v01, v01::GenericError};

static PRODUCT: LazyLock<ProductContext> =
	LazyLock::new(|| ProductContext::new(ProductId::for_test("product.dot")));

static OTHER_PRODUCT: LazyLock<ProductContext> =
	LazyLock::new(|| ProductContext::new(ProductId::for_test("other.dot")));

static PEOPL_APP: LazyLock<ProductContext> =
	LazyLock::new(|| ProductContext::new(ProductId::for_test("app.peopl.dot")));

static PEOPL: LazyLock<ProductContext> =
	LazyLock::new(|| ProductContext::new(ProductId::for_test("peopl.dot")));

#[derive(Default)]
struct MemStorage {
	inner: Mutex<HashMap<String, Vec<u8>>>,
}

#[async_trait::async_trait]
impl CoreStorage for MemStorage {
	async fn read_core_storage(
		&self,
		key: CoreStorageKey,
	) -> Result<Option<Vec<u8>>, v01::GenericError> {
		Ok(self.inner.lock().await.get(&test_key(key)).cloned())
	}
	async fn write_core_storage(
		&self,
		key: CoreStorageKey,
		value: Vec<u8>,
	) -> Result<(), v01::GenericError> {
		self.inner.lock().await.insert(test_key(key), value);
		Ok(())
	}
	async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), v01::GenericError> {
		self.inner.lock().await.remove(&test_key(key));
		Ok(())
	}
}

fn test_key(key: CoreStorageKey) -> String {
	hex::encode(key.encode())
}

#[test]
fn concurrent_remote_operations_recheck_the_first_decision() {
	futures::executor::block_on(async {
		for request in [
			remote_domains(&["cdn.example.com"]),
			RemotePermissionRequest { permission: RemotePermission::WebRtc },
		] {
			for (decision, first_status, second_status, calls) in [
				(
					PermissionDecision::AllowAlways,
					PermissionAuthorizationStatus::Authorized,
					PermissionAuthorizationStatus::Authorized,
					1,
				),
				(
					PermissionDecision::Deny,
					PermissionAuthorizationStatus::Denied,
					PermissionAuthorizationStatus::Denied,
					1,
				),
				(
					PermissionDecision::AllowOnce,
					PermissionAuthorizationStatus::Authorized,
					PermissionAuthorizationStatus::Denied,
					2,
				),
			] {
				let storage = MemStorage::default();
				let prompt =
					ScriptedPrompt::decisions(vec![], vec![PermissionDecision::Deny, decision]);
				let grants = Arc::default();
				let sdk = PermissionsService::new(&storage, &prompt, &PRODUCT)
					.with_temporary_permissions(Arc::clone(&grants));
				let native = PermissionsService::new(&storage, &prompt, &PRODUCT)
					.with_temporary_permissions(grants);
				let pending_answer = prompt.remote_answers.lock().await;
				let mut first = Box::pin(sdk.authorize_remote(request.clone()));
				let mut second = Box::pin(native.authorize_remote(request.clone()));

				assert!(futures::poll!(&mut first).is_pending());
				assert!(futures::poll!(&mut second).is_pending());
				drop(pending_answer);

				assert_eq!(
					(
						first.await.unwrap(),
						second.await.unwrap(),
						prompt.remote_calls.load(Ordering::SeqCst),
					),
					(first_status, second_status, calls),
				);
			}
		}
	});
}

#[test]
fn concurrent_device_operations_reuse_a_persisted_decision() {
	futures::executor::block_on(async {
		for (decision, expected) in [
			(PermissionDecision::AllowAlways, PermissionAuthorizationStatus::Authorized),
			(PermissionDecision::Deny, PermissionAuthorizationStatus::Denied),
		] {
			let storage = MemStorage::default();
			let prompt =
				ScriptedPrompt::decisions(vec![PermissionDecision::Deny, decision], vec![]);
			let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
			let pending_answer = prompt.device_answers.lock().await;
			let mut first = Box::pin(service.authorize_device(HostDevicePermissionRequest::Camera));
			let mut second =
				Box::pin(service.authorize_device(HostDevicePermissionRequest::Camera));

			assert!(futures::poll!(&mut first).is_pending());
			assert!(futures::poll!(&mut second).is_pending());
			drop(pending_answer);

			assert_eq!(
				(
					first.await.unwrap(),
					second.await.unwrap(),
					prompt.device_calls.load(Ordering::SeqCst),
				),
				(expected, expected, 1),
			);
		}
	});
}

#[test]
fn cancelling_a_prompt_releases_waiting_operations_without_blocking_other_products() {
	futures::executor::block_on(async {
		let storage = MemStorage::default();
		let prompt = ScriptedPrompt::decisions(vec![], vec![PermissionDecision::AllowAlways]);
		let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
		let request = remote_domains(&["cdn.example.com"]);
		let pending_answer = prompt.remote_answers.lock().await;
		let mut cancelled = Box::pin(service.authorize_remote(request.clone()));
		let mut waiting = Box::pin(service.authorize_remote(request.clone()));
		assert!(futures::poll!(&mut cancelled).is_pending());
		assert!(futures::poll!(&mut waiting).is_pending());

		let other_prompt = ScriptedPrompt::decisions(vec![], vec![PermissionDecision::AllowAlways]);
		let other = PermissionsService::new(&storage, &other_prompt, &OTHER_PRODUCT);
		let mut independent = Box::pin(other.authorize_remote(request));
		assert_eq!(
			futures::poll!(&mut independent),
			std::task::Poll::Ready(Ok(PermissionAuthorizationStatus::Authorized)),
		);

		drop(cancelled);
		drop(pending_answer);
		assert_eq!(
			(waiting.await.unwrap(), prompt.remote_calls.load(Ordering::SeqCst)),
			(PermissionAuthorizationStatus::Authorized, 2),
		);
	});
}

#[test]
fn an_upfront_one_use_grant_is_shared_with_one_operation_but_never_persisted() {
	futures::executor::block_on(async {
		let storage = MemStorage::default();
		let prompt = ScriptedPrompt::decisions(
			vec![],
			vec![PermissionDecision::Deny, PermissionDecision::AllowOnce],
		);
		let grants = Arc::default();
		let sdk = PermissionsService::new(&storage, &prompt, &PRODUCT)
			.with_temporary_permissions(Arc::clone(&grants));
		let operation = PermissionsService::new(&storage, &prompt, &PRODUCT)
			.with_temporary_permissions(Arc::clone(&grants));
		let other_product = PermissionsService::new(&storage, &prompt, &OTHER_PRODUCT)
			.with_temporary_permissions(grants);
		let other_execution = PermissionsService::new(&storage, &prompt, &PRODUCT);
		let request = remote_domains(&["api.example.com"]);
		let requested = sdk.check_or_prompt_remote(request.clone()).await.unwrap();
		assert_eq!(
			(
				requested,
				sdk.peek_remote(&request).await.unwrap(),
				other_product.peek_remote(&request).await.unwrap(),
				other_execution.peek_remote(&request).await.unwrap(),
				storage.inner.lock().await.len(),
			),
			(
				PermissionAuthorizationStatus::Authorized,
				PermissionAuthorizationStatus::Authorized,
				PermissionAuthorizationStatus::NotDetermined,
				PermissionAuthorizationStatus::NotDetermined,
				0,
			),
		);
		let first = operation.authorize_remote(request.clone()).await.unwrap();
		let second = operation.authorize_remote(request).await.unwrap();
		assert_eq!(
			(first, second, prompt.remote_calls.load(Ordering::SeqCst)),
			(PermissionAuthorizationStatus::Authorized, PermissionAuthorizationStatus::Denied, 2,),
		);
	});
}

#[test]
fn a_denied_bundle_preserves_one_use_grants_for_a_later_operation() {
	futures::executor::block_on(async {
		for prompt_denial in [false, true] {
			let storage = MemStorage::default();
			let prompt = ScriptedPrompt::decisions(
				vec![],
				vec![PermissionDecision::Deny, PermissionDecision::AllowOnce],
			);
			let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
			let granted = remote_domains(&["api.example.com"]);
			service.check_or_prompt_remote(granted.clone()).await.unwrap();
			if !prompt_denial {
				service
					.set_authorization_status(
						&PermissionAuthorizationRequest::Remote(remote_domains(&["blocked.com"])),
						PermissionAuthorizationStatus::Denied,
					)
					.await
					.unwrap();
			}
			let denied = service
				.authorize_remote(remote_domains(&["api.example.com", "blocked.com"]))
				.await
				.unwrap();
			let next = service
				.authorize_remote(remote_domains(&["api.example.com", "API.EXAMPLE.COM."]))
				.await
				.unwrap();
			assert_eq!(
				(denied, next, service.peek_remote(&granted).await.unwrap()),
				(
					PermissionAuthorizationStatus::Denied,
					PermissionAuthorizationStatus::Authorized,
					PermissionAuthorizationStatus::NotDetermined,
				),
			);
		}
	});
}

#[test]
fn concurrent_operations_cannot_share_a_one_use_wildcard_grant() {
	futures::executor::block_on(async {
		let storage = MemStorage::default();
		let prompt = ScriptedPrompt::decisions(
			vec![],
			vec![PermissionDecision::Deny, PermissionDecision::AllowOnce],
		);
		let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
		service
			.check_or_prompt_remote(remote_domains(&["*.example.com"]))
			.await
			.unwrap();
		let (first, second) = futures::join!(
			service.authorize_remote(remote_domains(&["deep.a.example.com"])),
			service.authorize_remote(remote_domains(&["deeper.deep.b.example.com"])),
		);
		assert_eq!(
			(first.unwrap(), second.unwrap(), prompt.remote_calls.load(Ordering::SeqCst)),
			(PermissionAuthorizationStatus::Authorized, PermissionAuthorizationStatus::Denied, 2,),
		);
	});
}

#[test]
fn a_one_use_answer_to_an_operation_is_consumed_by_that_operation() {
	futures::executor::block_on(async {
		let storage = MemStorage::default();
		let prompt = ScriptedPrompt::decisions(
			vec![PermissionDecision::AllowOnce],
			vec![PermissionDecision::AllowOnce],
		);
		let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
		let remote = remote_domains(&["api.example.com"]);
		let device = HostDevicePermissionRequest::Camera;
		assert_eq!(
			(
				service.authorize_remote(remote.clone()).await.unwrap(),
				service.peek_remote(&remote).await.unwrap(),
				service.authorize_device(device).await.unwrap(),
				service.peek_device(&device).await.unwrap(),
				storage.inner.lock().await.len(),
			),
			(
				PermissionAuthorizationStatus::Authorized,
				PermissionAuthorizationStatus::NotDetermined,
				PermissionAuthorizationStatus::Authorized,
				PermissionAuthorizationStatus::NotDetermined,
				0,
			),
		);
	});
}

#[test]
fn blessed_domains_need_no_prompt_but_explicit_denials_still_apply() {
	futures::executor::block_on(async {
		let storage = MemStorage::default();
		let prompt = ScriptedPrompt::new(vec![], vec![]);
		let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
		for domain in BLESSED_REMOTE_DOMAINS {
			let request = remote_domains(&[domain]);
			let granted = service.authorize_remote(request.clone()).await.unwrap();
			service
				.set_authorization_status(
					&PermissionAuthorizationRequest::Remote(request.clone()),
					PermissionAuthorizationStatus::Denied,
				)
				.await
				.unwrap();
			assert_eq!(
				(granted, service.authorize_remote(request).await.unwrap()),
				(PermissionAuthorizationStatus::Authorized, PermissionAuthorizationStatus::Denied,),
			);
		}
		assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 0);
	});
}

#[test]
fn blessed_domains_do_not_consume_a_one_use_grant_for_other_hosts() {
	futures::executor::block_on(async {
		let storage = MemStorage::default();
		let prompt = ScriptedPrompt::decisions(vec![], vec![PermissionDecision::AllowOnce]);
		let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
		let wildcard = remote_domains(&["*"]);
		service.check_or_prompt_remote(wildcard.clone()).await.unwrap();
		let blessed = service
			.authorize_remote(remote_domains(&[BLESSED_REMOTE_DOMAINS[0]]))
			.await
			.unwrap();
		let still_available = service.peek_remote(&wildcard).await.unwrap();
		let other = service.authorize_remote(remote_domains(&["other.com"])).await.unwrap();
		assert_eq!(
			(blessed, still_available, other, service.peek_remote(&wildcard).await.unwrap()),
			(
				PermissionAuthorizationStatus::Authorized,
				PermissionAuthorizationStatus::Authorized,
				PermissionAuthorizationStatus::Authorized,
				PermissionAuthorizationStatus::NotDetermined,
			),
		);
	});
}

#[test]
fn a_one_use_exception_to_a_denial_is_consumed_even_for_a_blessed_domain() {
	futures::executor::block_on(async {
		let storage = MemStorage::default();
		let prompt = ScriptedPrompt::decisions(vec![], vec![PermissionDecision::AllowOnce]);
		let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
		service
			.check_or_prompt_remote(remote_domains(&["*.googleapis.com"]))
			.await
			.unwrap();
		service
			.set_authorization_status(
				&PermissionAuthorizationRequest::Remote(remote_domains(&["*"])),
				PermissionAuthorizationStatus::Denied,
			)
			.await
			.unwrap();
		let request = remote_domains(&["fonts.googleapis.com"]);
		assert_eq!(
			(
				service.authorize_remote(request.clone()).await.unwrap(),
				service.authorize_remote(request).await.unwrap(),
				prompt.remote_calls.load(Ordering::SeqCst),
			),
			(PermissionAuthorizationStatus::Authorized, PermissionAuthorizationStatus::Denied, 1,),
		);
	});
}

#[test]
fn resetting_a_permission_removes_its_temporary_grant() {
	futures::executor::block_on(async {
		let storage = MemStorage::default();
		let prompt = ScriptedPrompt::decisions(vec![], vec![PermissionDecision::AllowOnce]);
		let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
		let request = remote_domains(&["api.example.com"]);
		service.check_or_prompt_remote(request.clone()).await.unwrap();
		service
			.set_authorization_status(
				&PermissionAuthorizationRequest::Remote(request.clone()),
				PermissionAuthorizationStatus::NotDetermined,
			)
			.await
			.unwrap();
		assert_eq!(
			service.peek_remote(&request).await.unwrap(),
			PermissionAuthorizationStatus::NotDetermined
		);
	});
}

struct ScriptedPrompt {
	device_answers: Mutex<Vec<PermissionDecision>>,
	remote_answers: Mutex<Vec<PermissionDecision>>,
	device_calls: AtomicUsize,
	remote_calls: AtomicUsize,
	/// Domain bundles the remote callback was actually asked about, in call
	/// order, so a test can assert which subset reached the user.
	remote_domains_asked: Mutex<Vec<Vec<String>>>,
}

impl ScriptedPrompt {
	fn new(device_answers: Vec<bool>, remote_answers: Vec<bool>) -> Self {
		let decision = |granted| {
			if granted { PermissionDecision::AllowAlways } else { PermissionDecision::Deny }
		};
		Self::decisions(
			device_answers.into_iter().map(decision).collect(),
			remote_answers.into_iter().map(decision).collect(),
		)
	}

	fn decisions(
		device_answers: Vec<PermissionDecision>,
		remote_answers: Vec<PermissionDecision>,
	) -> Self {
		Self {
			device_answers: Mutex::new(device_answers),
			remote_answers: Mutex::new(remote_answers),
			device_calls: AtomicUsize::new(0),
			remote_calls: AtomicUsize::new(0),
			remote_domains_asked: Mutex::new(Vec::new()),
		}
	}

	fn domains_asked(&self) -> Vec<Vec<String>> {
		futures::executor::block_on(self.remote_domains_asked.lock()).clone()
	}
}

#[async_trait::async_trait]
impl Permissions for ScriptedPrompt {
	async fn device_permission(
		&self,
		_product: &ProductContext,
		_request: HostDevicePermissionRequest,
	) -> Result<PermissionDecision, GenericError> {
		self.device_calls.fetch_add(1, Ordering::SeqCst);
		let decision = self
			.device_answers
			.lock()
			.await
			.pop()
			.expect("ScriptedPrompt ran out of device answers");
		Ok(decision)
	}

	async fn remote_permission(
		&self,
		_product: &ProductContext,
		request: RemotePermissionRequest,
	) -> Result<PermissionDecision, GenericError> {
		self.remote_calls.fetch_add(1, Ordering::SeqCst);
		if let RemotePermission::Remote { domains } = &request.permission {
			self.remote_domains_asked.lock().await.push(domains.clone());
		}
		let decision = self
			.remote_answers
			.lock()
			.await
			.pop()
			.expect("ScriptedPrompt ran out of remote answers");
		Ok(decision)
	}
}

/// OS status source, scripted per capability so a test can deny one
/// capability while another stays granted.
struct ScriptedStatus {
	answers: Vec<(HostDevicePermissionRequest, DevicePermissionStatus)>,
	/// Answer for any capability `answers` does not name. `None` fails the
	/// query instead, standing in for a busy host or a dropped IPC.
	fallback: Option<DevicePermissionStatus>,
	asked: Mutex<Vec<HostDevicePermissionRequest>>,
}

impl ScriptedStatus {
	fn always(status: DevicePermissionStatus) -> Self {
		Self { answers: Vec::new(), fallback: Some(status), asked: Mutex::new(Vec::new()) }
	}

	fn per_capability(
		answers: Vec<(HostDevicePermissionRequest, DevicePermissionStatus)>,
		fallback: DevicePermissionStatus,
	) -> Self {
		Self { answers, fallback: Some(fallback), asked: Mutex::new(Vec::new()) }
	}

	fn failing() -> Self {
		Self { answers: Vec::new(), fallback: None, asked: Mutex::new(Vec::new()) }
	}

	fn asked(&self) -> Vec<HostDevicePermissionRequest> {
		futures::executor::block_on(self.asked.lock()).clone()
	}
}

#[async_trait::async_trait]
impl PermissionStatusHost for ScriptedStatus {
	async fn device_permission_status(
		&self,
		request: HostDevicePermissionRequest,
	) -> Result<DevicePermissionStatus, GenericError> {
		self.asked.lock().await.push(request);
		if let Some((_, status)) =
			self.answers.iter().find(|(capability, _)| *capability == request)
		{
			return Ok(*status);
		}
		self.fallback
			.ok_or_else(|| v01::GenericError { reason: "status channel unavailable".to_string() })
	}
}

/// Persist a product-scoped grant for `capability` the way a first
/// successful request does, with no OS status source involved.
fn grant_stored(storage: &MemStorage, capability: HostDevicePermissionRequest) {
	let prompt = ScriptedPrompt::new(vec![true], vec![]);
	let service = PermissionsService::new(storage, &prompt, &PRODUCT);
	assert_eq!(
		futures::executor::block_on(service.check_or_prompt_device(capability)).unwrap(),
		PermissionAuthorizationStatus::Authorized,
	);
}

fn remote_domains(domains: &[&str]) -> RemotePermissionRequest {
	RemotePermissionRequest {
		permission: RemotePermission::Remote {
			domains: domains.iter().map(|domain| domain.to_string()).collect(),
		},
	}
}

#[test]
fn check_or_prompt_device_caches_grant() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![true], vec![]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	let first = futures::executor::block_on(
		service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
	)
	.unwrap();
	let second = futures::executor::block_on(
		service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
	)
	.unwrap();

	assert_eq!(first, PermissionAuthorizationStatus::Authorized);
	assert_eq!(second, PermissionAuthorizationStatus::Authorized);
	assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn check_or_prompt_remote_caches_denial() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![false]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	let request = RemotePermissionRequest { permission: RemotePermission::ChainSubmit };
	let first =
		futures::executor::block_on(service.check_or_prompt_remote(request.clone())).unwrap();
	let second = futures::executor::block_on(service.check_or_prompt_remote(request)).unwrap();

	assert_eq!(first, PermissionAuthorizationStatus::Denied);
	assert_eq!(second, PermissionAuthorizationStatus::Denied);
	assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
}

/// The defect this storage model exists to fix: a product grants a bundle,
/// then enforcement asks about one host in it. Keying the bundle as a set
/// made that lookup miss and re-prompt, so a granted domain read as
/// undecided forever.
#[test]
fn a_bundle_grant_is_visible_to_a_single_domain_lookup() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![true]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	let granted = futures::executor::block_on(
		service.check_or_prompt_remote(remote_domains(&["a.example.com", "b.example.com"])),
	)
	.unwrap();
	assert_eq!(granted, PermissionAuthorizationStatus::Authorized);

	for domain in ["a.example.com", "b.example.com"] {
		assert_eq!(
			futures::executor::block_on(service.peek_remote(&remote_domains(&[domain]))).unwrap(),
			PermissionAuthorizationStatus::Authorized,
			"{domain} was granted as part of the bundle"
		);
		assert_eq!(
			futures::executor::block_on(service.check_or_prompt_remote(remote_domains(&[domain])))
				.unwrap(),
			PermissionAuthorizationStatus::Authorized,
		);
	}
	assert_eq!(
		prompt.remote_calls.load(Ordering::SeqCst),
		1,
		"a domain already covered by the bundle must not re-prompt"
	);
}

#[test]
fn a_wildcard_grant_covers_descendants_but_not_its_root() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![true]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	futures::executor::block_on(service.check_or_prompt_remote(remote_domains(&["*.example.com"])))
		.unwrap();

	for covered in ["api.example.com", "deep.api.example.com", "deeper.deep.api.example.com"] {
		assert_eq!(
			futures::executor::block_on(service.peek_remote(&remote_domains(&[covered]))).unwrap(),
			PermissionAuthorizationStatus::Authorized,
			"{covered} is a descendant of example.com"
		);
	}
	for uncovered in ["example.com", "notexample.com", "example.com.other"] {
		assert_eq!(
			futures::executor::block_on(service.peek_remote(&remote_domains(&[uncovered])))
				.unwrap(),
			PermissionAuthorizationStatus::NotDetermined,
			"{uncovered} is not a descendant of example.com"
		);
	}
}

#[test]
fn the_most_specific_stored_decision_wins() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	// Deny the whole wildcard, then allow one host under it explicitly.
	futures::executor::block_on(service.set_authorization_status(
		&PermissionAuthorizationRequest::Remote(remote_domains(&["*.example.com"])),
		PermissionAuthorizationStatus::Denied,
	))
	.unwrap();
	futures::executor::block_on(service.set_authorization_status(
		&PermissionAuthorizationRequest::Remote(remote_domains(&["api.example.com"])),
		PermissionAuthorizationStatus::Authorized,
	))
	.unwrap();

	assert_eq!(
		futures::executor::block_on(service.peek_remote(&remote_domains(&["api.example.com"])))
			.unwrap(),
		PermissionAuthorizationStatus::Authorized,
		"an explicit host grant outranks a denied parent wildcard"
	);
	assert_eq!(
		futures::executor::block_on(service.peek_remote(&remote_domains(&["other.example.com"])))
			.unwrap(),
		PermissionAuthorizationStatus::Denied,
		"a host with no decision of its own inherits the wildcard denial"
	);

	futures::executor::block_on(service.set_authorization_status(
		&PermissionAuthorizationRequest::Remote(remote_domains(&["*.example.com"])),
		PermissionAuthorizationStatus::Authorized,
	))
	.unwrap();
	futures::executor::block_on(service.set_authorization_status(
		&PermissionAuthorizationRequest::Remote(remote_domains(&["*.api.example.com"])),
		PermissionAuthorizationStatus::Denied,
	))
	.unwrap();
	for denied in ["deep.api.example.com", "deeper.deep.api.example.com"] {
		assert_eq!(
			futures::executor::block_on(service.peek_remote(&remote_domains(&[denied]))).unwrap(),
			PermissionAuthorizationStatus::Denied,
			"the narrower wildcard denial overrides the ancestor's grant"
		);
	}
}

#[test]
fn a_prompt_covers_only_undetermined_domains_and_never_revokes_a_grant() {
	let storage = MemStorage::default();
	// Answers pop from the end: grant first, then deny.
	let prompt = ScriptedPrompt::new(vec![], vec![false, true]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	futures::executor::block_on(service.check_or_prompt_remote(remote_domains(&["a.com"])))
		.unwrap();
	let second = futures::executor::block_on(
		service.check_or_prompt_remote(remote_domains(&["a.com", "b.com"])),
	)
	.unwrap();

	assert_eq!(
		prompt.domains_asked(),
		vec![vec!["a.com".to_string()], vec!["b.com".to_string()]],
		"the second prompt must ask only about the undecided domain"
	);
	assert_eq!(
		second,
		PermissionAuthorizationStatus::Denied,
		"the bundle needs every domain, so one denial denies it"
	);
	assert_eq!(
		futures::executor::block_on(service.peek_remote(&remote_domains(&["a.com"]))).unwrap(),
		PermissionAuthorizationStatus::Authorized,
		"denying b.com must not revoke the existing a.com grant"
	);
}

#[test]
fn a_multi_domain_denial_leaves_the_narrower_question_askable() {
	let storage = MemStorage::default();
	// Answers pop from the end: deny the pair, then grant the single domain.
	let prompt = ScriptedPrompt::new(vec![], vec![true, false]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	let pair = remote_domains(&["api.coingecko.com", "analytics.vendor.com"]);
	assert_eq!(
		futures::executor::block_on(service.check_or_prompt_remote(pair.clone())).unwrap(),
		PermissionAuthorizationStatus::Denied
	);
	assert_eq!(
		futures::executor::block_on(service.check_or_prompt_remote(pair.clone())).unwrap(),
		PermissionAuthorizationStatus::Denied,
	);
	assert_eq!(
		prompt.remote_calls.load(Ordering::SeqCst),
		1,
		"the refused set is answered and must not be re-asked"
	);

	// Refusing the pair is not refusing either domain on its own: that is a
	// question the user was never put, so it stays open.
	for domain in ["api.coingecko.com", "analytics.vendor.com"] {
		assert_eq!(
			futures::executor::block_on(service.peek_remote(&remote_domains(&[domain]))).unwrap(),
			PermissionAuthorizationStatus::NotDetermined,
			"{domain} was never refused by itself"
		);
	}
	assert_eq!(
		futures::executor::block_on(
			service.check_or_prompt_remote(remote_domains(&["api.coingecko.com"]))
		)
		.unwrap(),
		PermissionAuthorizationStatus::Authorized,
	);
	assert_eq!(
		prompt.domains_asked(),
		vec![
			vec!["api.coingecko.com".to_string(), "analytics.vendor.com".to_string(),],
			vec!["api.coingecko.com".to_string()],
		],
	);
}

#[test]
fn unsupported_wildcards_are_rejected_without_prompting_or_storing() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	for domain in ["*.com", "*.dot", "*.127.0.0.1", "*.[::1]"] {
		assert_eq!(
			futures::executor::block_on(service.check_or_prompt_remote(remote_domains(&[domain])))
				.unwrap(),
			PermissionAuthorizationStatus::Denied,
		);
	}
	assert_eq!(
		(
			prompt.remote_calls.load(Ordering::SeqCst),
			futures::executor::block_on(storage.inner.lock()).len()
		),
		(0, 0),
	);
}

#[test]
fn a_grant_covers_every_spelling_of_the_granted_host() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![true]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	futures::executor::block_on(
		service.check_or_prompt_remote(remote_domains(&["Bücher.example"])),
	)
	.unwrap();

	// Enforcement normalizes a live URL host the same way the grant was
	// keyed, so no spelling of the same site opens a second prompt.
	for spelling in ["bücher.example", "xn--bcher-kva.example", "XN--BCHER-KVA.example."] {
		assert_eq!(
			futures::executor::block_on(service.peek_remote(&remote_domains(&[spelling]))).unwrap(),
			PermissionAuthorizationStatus::Authorized,
			"{spelling} is the granted host"
		);
	}
	assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_denied_domain_short_circuits_the_bundle_without_prompting() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![false]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	futures::executor::block_on(service.check_or_prompt_remote(remote_domains(&["a.com"])))
		.unwrap();
	let second = futures::executor::block_on(
		service.check_or_prompt_remote(remote_domains(&["a.com", "b.com"])),
	)
	.unwrap();

	assert_eq!(second, PermissionAuthorizationStatus::Denied);
	assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1, "a stored denial is not re-asked");
}

#[test]
fn an_empty_domain_bundle_is_denied_without_prompting() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	assert_eq!(
		futures::executor::block_on(service.check_or_prompt_remote(remote_domains(&[]))).unwrap(),
		PermissionAuthorizationStatus::Denied
	);
	assert_eq!(
		futures::executor::block_on(service.peek_remote(&remote_domains(&[]))).unwrap(),
		PermissionAuthorizationStatus::Denied
	);
	assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn clearing_a_bundle_clears_each_domain_it_names() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![true]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	futures::executor::block_on(
		service.check_or_prompt_remote(remote_domains(&["a.com", "b.com"])),
	)
	.unwrap();
	futures::executor::block_on(service.set_authorization_status(
		&PermissionAuthorizationRequest::Remote(remote_domains(&["a.com", "b.com"])),
		PermissionAuthorizationStatus::NotDetermined,
	))
	.unwrap();

	for domain in ["a.com", "b.com"] {
		assert_eq!(
			futures::executor::block_on(service.peek_remote(&remote_domains(&[domain]))).unwrap(),
			PermissionAuthorizationStatus::NotDetermined,
			"{domain} must be revocable through the bundle it was granted in"
		);
	}
}

#[test]
fn resetting_a_bundle_clears_a_recorded_denial() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![false]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
	let pair = remote_domains(&["a.com", "b.com"]);

	futures::executor::block_on(service.check_or_prompt_remote(pair.clone())).unwrap();
	futures::executor::block_on(service.set_authorization_status(
		&PermissionAuthorizationRequest::Remote(pair.clone()),
		PermissionAuthorizationStatus::NotDetermined,
	))
	.unwrap();

	assert_eq!(
		futures::executor::block_on(service.peek_remote(&pair)).unwrap(),
		PermissionAuthorizationStatus::NotDetermined,
		"the host's reset must also reach the slot a set denial lives in"
	);
}

#[test]
fn remote_domain_grants_are_scoped_to_one_product() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![true]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
	futures::executor::block_on(service.check_or_prompt_remote(remote_domains(&["a.com"])))
		.unwrap();

	let other = PermissionsService::new(&storage, &prompt, &OTHER_PRODUCT);
	assert_eq!(
		futures::executor::block_on(other.peek_remote(&remote_domains(&["a.com"]))).unwrap(),
		PermissionAuthorizationStatus::NotDetermined
	);
}

/// A trusted product, so `ScriptedPrompt` is built with no scripted answers:
/// reaching either callback panics rather than silently answering.
fn trusted_service<'a>(
	storage: &'a MemStorage,
	prompt: &'a ScriptedPrompt,
) -> PermissionsService<'a, MemStorage, ScriptedPrompt> {
	PermissionsService::new(storage, prompt, &PEOPL)
}

fn remote(permission: RemotePermission) -> RemotePermissionRequest {
	RemotePermissionRequest { permission }
}

fn every_remote_permission() -> Vec<RemotePermission> {
	vec![
		RemotePermission::Remote { domains: vec!["example.com".to_string()] },
		RemotePermission::WebRtc,
		RemotePermission::ChainSubmit,
		RemotePermission::PreimageSubmit,
		RemotePermission::StatementSubmit,
		RemotePermission::BalanceAccess,
		RemotePermission::NftPurseRead { owner: v01::NftPurseOwner::Root },
	]
}

#[test]
fn a_trusted_product_holds_every_remote_permission_without_prompting() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = trusted_service(&storage, &prompt);

	for permission in every_remote_permission() {
		assert_eq!(
			futures::executor::block_on(service.check_or_prompt_remote(remote(permission.clone())))
				.unwrap(),
			PermissionAuthorizationStatus::Authorized,
			"{permission:?} must be granted to a trusted product without a prompt"
		);
	}
	assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 0);
	assert!(prompt.domains_asked().is_empty());
}

#[test]
fn a_trusted_product_is_authorized_for_any_domain() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = trusted_service(&storage, &prompt);

	for domains in
		[vec!["a.com"], vec!["deep.api.example.com"], vec!["*"], vec!["a.com", "b.com", "*.c.com"]]
	{
		assert_eq!(
			futures::executor::block_on(service.peek_remote(&remote_domains(&domains))).unwrap(),
			PermissionAuthorizationStatus::Authorized,
			"{domains:?} must be authorized for a trusted product"
		);
	}
}

#[test]
fn a_trusted_product_reports_authorized_to_the_admin_surface() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = trusted_service(&storage, &prompt);

	for request in [remote(RemotePermission::ChainSubmit), remote_domains(&["a.com"])] {
		assert_eq!(
			futures::executor::block_on(
				service.authorization_status(&PermissionAuthorizationRequest::Remote(request))
			)
			.unwrap(),
			PermissionAuthorizationStatus::Authorized
		);
	}
}

#[test]
fn a_trusted_product_grant_is_not_persisted() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = trusted_service(&storage, &prompt);
	let request = remote(RemotePermission::ChainSubmit);

	futures::executor::block_on(service.check_or_prompt_remote(request.clone())).unwrap();

	assert_eq!(
		futures::executor::block_on(storage.read_core_storage(
			CoreStorageKey::remote_permission_authorization(
				&ProductId::for_test("peopl.dot"),
				&request
			)
		))
		.unwrap(),
		None,
		"trusted authorization must not create a permission record"
	);
}

#[test]
fn a_trusted_product_ignores_stored_remote_denials() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = trusted_service(&storage, &prompt);

	for request in [remote(RemotePermission::ChainSubmit), remote_domains(&["a.com"])] {
		futures::executor::block_on(service.set_authorization_status(
			&PermissionAuthorizationRequest::Remote(request.clone()),
			PermissionAuthorizationStatus::Denied,
		))
		.unwrap();

		assert_eq!(
			futures::executor::block_on(service.peek_remote(&request)).unwrap(),
			PermissionAuthorizationStatus::Authorized
		);
		assert_eq!(
			futures::executor::block_on(service.check_or_prompt_remote(request)).unwrap(),
			PermissionAuthorizationStatus::Authorized
		);
	}
	assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn a_trusted_product_ignores_stored_wildcard_denials() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = trusted_service(&storage, &prompt);

	futures::executor::block_on(service.set_authorization_status(
		&PermissionAuthorizationRequest::Remote(remote_domains(&["*"])),
		PermissionAuthorizationStatus::Denied,
	))
	.unwrap();

	for domain in ["a.com", "deep.api.example.com"] {
		assert_eq!(
			futures::executor::block_on(service.peek_remote(&remote_domains(&[domain]))).unwrap(),
			PermissionAuthorizationStatus::Authorized,
			"stored wildcard decisions do not govern trusted products"
		);
	}
}

#[test]
fn a_trusted_label_on_every_product_network_is_trusted() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);

	for product_id in ["peopl.dot", "peopl.paseo", "peopl.testnet", "dim2.dot", "stash.dot"] {
		let product = ProductContext::new(ProductId::for_test(product_id));
		let service = PermissionsService::new(&storage, &prompt, &product);
		assert_eq!(
			futures::executor::block_on(
				service.peek_remote(&remote(RemotePermission::ChainSubmit))
			)
			.unwrap(),
			PermissionAuthorizationStatus::Authorized,
			"{product_id} must be trusted on every accepted product network"
		);
	}
}

#[test]
fn a_subdomain_of_a_trusted_label_is_not_trusted() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![true]);
	let service = PermissionsService::new(&storage, &prompt, &PEOPL_APP);
	let request = remote(RemotePermission::ChainSubmit);

	assert_eq!(
		futures::executor::block_on(service.peek_remote(&request)).unwrap(),
		PermissionAuthorizationStatus::NotDetermined
	);
	assert_eq!(
		futures::executor::block_on(service.check_or_prompt_remote(request)).unwrap(),
		PermissionAuthorizationStatus::Authorized
	);
	assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_localhost_product_is_not_trusted() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![true, true]);

	for product_id in ["localhost", "localhost:3000"] {
		let product = ProductContext::new(ProductId::for_test(product_id));
		let service = PermissionsService::new(&storage, &prompt, &product);
		let request = remote(RemotePermission::ChainSubmit);
		assert_eq!(
			futures::executor::block_on(service.peek_remote(&request)).unwrap(),
			PermissionAuthorizationStatus::NotDetermined,
			"{product_id} carries no label to match and must prompt"
		);
		futures::executor::block_on(service.check_or_prompt_remote(request)).unwrap();
	}
	assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 2);
}

#[test]
fn an_untrusted_product_still_prompts_for_every_remote_permission() {
	let storage = MemStorage::default();
	let permissions = every_remote_permission();
	let prompt = ScriptedPrompt::new(vec![], vec![true; permissions.len()]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	for permission in permissions.iter().cloned() {
		futures::executor::block_on(service.check_or_prompt_remote(remote(permission))).unwrap();
	}
	assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), permissions.len());
}

#[test]
fn a_trusted_product_holds_every_device_permission_without_prompting() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = trusted_service(&storage, &prompt);

	for &permission in HostDevicePermissionRequest::VARIANTS {
		assert_eq!(
			[
				futures::executor::block_on(service.check_or_prompt_device(permission)).unwrap(),
				futures::executor::block_on(service.authorize_device(permission)).unwrap(),
			],
			[PermissionAuthorizationStatus::Authorized; 2],
			"{permission:?} must be granted to a trusted product without a prompt"
		);
	}
	assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn a_trusted_product_is_denied_a_device_capability_the_os_refuses() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let status = ScriptedStatus::per_capability(
		vec![
			(HostDevicePermissionRequest::Notifications, DevicePermissionStatus::Denied),
			(HostDevicePermissionRequest::Camera, DevicePermissionStatus::NotDetermined),
		],
		DevicePermissionStatus::Granted,
	);
	let service = trusted_service(&storage, &prompt).with_status_host(Some(&status));
	let request = |permission| {
		futures::executor::block_on(service.check_or_prompt_device(permission)).unwrap()
	};

	assert_eq!(
		[
			request(HostDevicePermissionRequest::Notifications),
			request(HostDevicePermissionRequest::Camera),
			request(HostDevicePermissionRequest::Microphone),
		],
		[
			PermissionAuthorizationStatus::Denied,
			// The OS asks its own question where the capability is used.
			PermissionAuthorizationStatus::Authorized,
			PermissionAuthorizationStatus::Authorized,
		],
	);
	assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn a_trusted_product_reads_no_permission_storage() {
	let storage = FailingStorage;
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = PermissionsService::new(&storage, &prompt, &PEOPL);
	let status = |request| futures::executor::block_on(service.authorization_status(&request));
	assert_eq!(
		[
			status(PermissionAuthorizationRequest::Remote(remote(RemotePermission::ChainSubmit))),
			status(PermissionAuthorizationRequest::IdentityDisclosure),
			status(PermissionAuthorizationRequest::AccountAccess {
				target_product_id: ProductId::for_test("other.dot"),
			}),
			status(PermissionAuthorizationRequest::Device(HostDevicePermissionRequest::Camera)),
			futures::executor::block_on(
				service.check_or_prompt_device(HostDevicePermissionRequest::Notifications)
			),
		],
		[
			Ok(PermissionAuthorizationStatus::Authorized),
			Ok(PermissionAuthorizationStatus::Authorized),
			Ok(PermissionAuthorizationStatus::Authorized),
			Ok(PermissionAuthorizationStatus::Authorized),
			Ok(PermissionAuthorizationStatus::Authorized),
		],
	);
}

#[test]
fn an_empty_domain_bundle_is_denied_for_a_trusted_product() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = trusted_service(&storage, &prompt);

	assert_eq!(
		futures::executor::block_on(service.peek_remote(&remote_domains(&[]))).unwrap(),
		PermissionAuthorizationStatus::Denied
	);
	assert_eq!(
		futures::executor::block_on(service.check_or_prompt_remote(remote_domains(&[]))).unwrap(),
		PermissionAuthorizationStatus::Denied,
		"an empty bundle grants nothing, so failing closed outranks the whitelist"
	);
	assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn device_and_remote_caches_are_independent() {
	let storage = MemStorage::default();
	// Device denies, remote grants. If the caches collided we'd see the
	// same answer on the second call.
	let prompt = ScriptedPrompt::new(vec![false], vec![true]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	let device = futures::executor::block_on(
		service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
	)
	.unwrap();
	let remote =
		futures::executor::block_on(service.check_or_prompt_remote(RemotePermissionRequest {
			permission: RemotePermission::ChainSubmit,
		}))
		.unwrap();

	assert_eq!(device, PermissionAuthorizationStatus::Denied);
	assert_eq!(remote, PermissionAuthorizationStatus::Authorized);
	assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
	assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn device_prompt_does_not_invoke_remote_callback() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![true], vec![]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	let _ = futures::executor::block_on(
		service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
	)
	.unwrap();
	assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
	assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn remote_prompt_does_not_invoke_device_callback() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![true]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	let _ =
		futures::executor::block_on(service.check_or_prompt_remote(RemotePermissionRequest {
			permission: RemotePermission::WebRtc,
		}))
		.unwrap();
	assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 0);
	assert_eq!(prompt.remote_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn peek_returns_not_determined_until_authorized() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![true], vec![]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	let before =
		futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
			.unwrap();
	assert_eq!(before, PermissionAuthorizationStatus::NotDetermined);

	futures::executor::block_on(
		service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
	)
	.unwrap();

	let after =
		futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
			.unwrap();
	assert_eq!(after, PermissionAuthorizationStatus::Authorized);
}

#[test]
fn set_authorization_status_writes_and_clears() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
	let request = PermissionAuthorizationRequest::Device(HostDevicePermissionRequest::Camera);

	futures::executor::block_on(
		service.set_authorization_status(&request, PermissionAuthorizationStatus::Authorized),
	)
	.unwrap();
	assert_eq!(
		futures::executor::block_on(service.authorization_status(&request)).unwrap(),
		PermissionAuthorizationStatus::Authorized
	);

	futures::executor::block_on(
		service.set_authorization_status(&request, PermissionAuthorizationStatus::NotDetermined),
	)
	.unwrap();
	assert_eq!(
		futures::executor::block_on(service.authorization_status(&request)).unwrap(),
		PermissionAuthorizationStatus::NotDetermined
	);
}

#[test]
fn identity_disclosure_authorization_round_trips() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
	let request = PermissionAuthorizationRequest::IdentityDisclosure;

	assert_eq!(
		futures::executor::block_on(service.authorization_status(&request)).unwrap(),
		PermissionAuthorizationStatus::NotDetermined
	);

	futures::executor::block_on(
		service.set_authorization_status(&request, PermissionAuthorizationStatus::Authorized),
	)
	.unwrap();
	assert_eq!(
		futures::executor::block_on(service.authorization_status(&request)).unwrap(),
		PermissionAuthorizationStatus::Authorized
	);

	let other_product_service = PermissionsService::new(&storage, &prompt, &OTHER_PRODUCT);
	assert_eq!(
		futures::executor::block_on(other_product_service.authorization_status(&request)).unwrap(),
		PermissionAuthorizationStatus::NotDetermined
	);
}

#[test]
fn account_access_authorization_is_scoped_by_requester_and_target() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);
	let request = PermissionAuthorizationRequest::AccountAccess {
		target_product_id: ProductId::for_test("target.dot"),
	};

	futures::executor::block_on(
		service.set_authorization_status(&request, PermissionAuthorizationStatus::Authorized),
	)
	.unwrap();
	assert_eq!(
		futures::executor::block_on(service.authorization_status(&request)).unwrap(),
		PermissionAuthorizationStatus::Authorized
	);
	assert_eq!(
		futures::executor::block_on(service.authorization_status(
			&PermissionAuthorizationRequest::AccountAccess {
				target_product_id: ProductId::for_test("other.dot"),
			}
		))
		.unwrap(),
		PermissionAuthorizationStatus::NotDetermined
	);

	let other_product_service = PermissionsService::new(&storage, &prompt, &OTHER_PRODUCT);
	assert_eq!(
		futures::executor::block_on(other_product_service.authorization_status(&request)).unwrap(),
		PermissionAuthorizationStatus::NotDetermined
	);
}

/// Prompt callback that always errors, to exercise the transient-failure
/// path (fail closed for the current call, but do not persist the error).
struct FailingPrompt;

#[async_trait::async_trait]
impl Permissions for FailingPrompt {
	async fn device_permission(
		&self,
		_product: &ProductContext,
		_request: HostDevicePermissionRequest,
	) -> Result<PermissionDecision, GenericError> {
		Err(GenericError { reason: "boom".into() })
	}

	async fn remote_permission(
		&self,
		_product: &ProductContext,
		_request: RemotePermissionRequest,
	) -> Result<PermissionDecision, GenericError> {
		Err(GenericError { reason: "boom".into() })
	}
}

#[test]
fn prompt_failure_stays_not_determined_without_persisting() {
	let storage = MemStorage::default();
	let prompt = FailingPrompt;
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	let device_decision = futures::executor::block_on(
		service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
	)
	.unwrap();
	assert_eq!(device_decision, PermissionAuthorizationStatus::NotDetermined);

	let remote_request = RemotePermissionRequest { permission: RemotePermission::ChainSubmit };
	let remote_decision =
		futures::executor::block_on(service.check_or_prompt_remote(remote_request.clone()))
			.unwrap();
	assert_eq!(remote_decision, PermissionAuthorizationStatus::NotDetermined);

	// A transient callback error is not cached, so peek still sees no
	// authorization and the next request re-prompts rather than
	// permanently locking out the capability.
	let cached_device =
		futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
			.unwrap();
	assert_eq!(
		cached_device,
		PermissionAuthorizationStatus::NotDetermined,
		"a transient prompt error must not be persisted"
	);
	let cached_remote = futures::executor::block_on(service.peek_remote(&remote_request)).unwrap();
	assert_eq!(
		cached_remote,
		PermissionAuthorizationStatus::NotDetermined,
		"a transient prompt error must not be persisted"
	);
}

/// A corrupt SCALE-encoded cache entry must be treated as "no cache",
/// not panic. The service falls back to prompting.
#[test]
fn corrupt_cache_entry_returns_none() {
	let storage = MemStorage::default();
	// Write garbage bytes under the canonical key.
	futures::executor::block_on(storage.write_core_storage(
		CoreStorageKey::device_permission_authorization(
			&ProductId::for_test("product.dot"),
			&HostDevicePermissionRequest::Camera,
		),
		vec![0xff, 0xfe, 0xfd],
	))
	.unwrap();

	let prompt = ScriptedPrompt::new(vec![true], vec![]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	let peeked =
		futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
			.unwrap();
	assert_eq!(
		peeked,
		PermissionAuthorizationStatus::NotDetermined,
		"corrupt entry must decode as absent"
	);
}

/// Storage failures must propagate to the caller; the service must not
/// swallow them by silently returning a default authorization.
#[derive(Default)]
struct FailingStorage;

#[async_trait::async_trait]
impl CoreStorage for FailingStorage {
	async fn read_core_storage(
		&self,
		_key: CoreStorageKey,
	) -> Result<Option<Vec<u8>>, v01::GenericError> {
		Err(v01::GenericError { reason: "read failed".into() })
	}
	async fn write_core_storage(
		&self,
		_key: CoreStorageKey,
		_value: Vec<u8>,
	) -> Result<(), v01::GenericError> {
		Err(v01::GenericError { reason: "write failed".into() })
	}
	async fn clear_core_storage(&self, _key: CoreStorageKey) -> Result<(), v01::GenericError> {
		Err(v01::GenericError { reason: "clear failed".into() })
	}
}

#[test]
fn storage_read_error_propagates() {
	let storage = FailingStorage;
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT);

	let err = futures::executor::block_on(
		service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
	)
	.expect_err("read failure must surface");
	assert!(matches!(err, v01::GenericError { .. }));
}

#[test]
fn an_os_denial_overrides_a_stored_grant() {
	let storage = MemStorage::default();
	grant_stored(&storage, HostDevicePermissionRequest::Camera);

	// No device answers scripted: reaching the prompt at all would panic,
	// which is the assertion that a settings-level refusal is not something
	// the user can be asked about.
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let status = ScriptedStatus::always(DevicePermissionStatus::Denied);
	let service =
		PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&status));

	assert_eq!(
		futures::executor::block_on(
			service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
		)
		.unwrap(),
		PermissionAuthorizationStatus::Denied,
	);
}

#[test]
fn an_os_denial_leaves_the_stored_grant_intact() {
	let storage = MemStorage::default();
	grant_stored(&storage, HostDevicePermissionRequest::Camera);

	let denied_prompt = ScriptedPrompt::new(vec![], vec![]);
	let denied = ScriptedStatus::always(DevicePermissionStatus::Denied);
	futures::executor::block_on(
		PermissionsService::new(&storage, &denied_prompt, &PRODUCT)
			.with_status_host(Some(&denied))
			.check_or_prompt_device(HostDevicePermissionRequest::Camera),
	)
	.unwrap();

	// The user restores the OS grant in settings. The product decision was
	// never theirs to lose, so this resolves without asking them again.
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let restored = ScriptedStatus::always(DevicePermissionStatus::Granted);
	let service =
		PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&restored));

	assert_eq!(
		futures::executor::block_on(
			service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
		)
		.unwrap(),
		PermissionAuthorizationStatus::Authorized,
	);
	assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn an_os_reset_never_reprompts_a_stored_grant() {
	let storage = MemStorage::default();
	grant_stored(&storage, HostDevicePermissionRequest::Camera);

	// Android auto-resets runtime permissions for unused apps. The core
	// cannot ask the OS on its own — the prompt callback also puts the
	// product's question to the user — so it does not try. No answers are
	// scripted: reaching the prompt at all would panic.
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let status = ScriptedStatus::always(DevicePermissionStatus::NotDetermined);
	let service =
		PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&status));

	// Repeated, because a condition a prompt cannot clear re-fires forever.
	for _ in 0..3 {
		assert_eq!(
			futures::executor::block_on(
				service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
			)
			.unwrap(),
			PermissionAuthorizationStatus::Authorized,
		);
	}
	assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn an_os_reset_cannot_turn_a_stored_grant_into_a_permanent_denial() {
	let storage = MemStorage::default();
	grant_stored(&storage, HostDevicePermissionRequest::Camera);

	// On iOS and Android the prompt callback *is* the OS dialog, so a
	// "Don't Allow" answered there is an answer about the OS, not about the
	// product. Persisting it would replace the product's grant, and
	// restoring the capability in system settings could never recover it.
	let declining = ScriptedPrompt::new(vec![false], vec![]);
	let reset = ScriptedStatus::always(DevicePermissionStatus::NotDetermined);
	futures::executor::block_on(
		PermissionsService::new(&storage, &declining, &PRODUCT)
			.with_status_host(Some(&reset))
			.check_or_prompt_device(HostDevicePermissionRequest::Camera),
	)
	.unwrap();

	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let restored = ScriptedStatus::always(DevicePermissionStatus::Granted);
	let after =
		PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&restored));
	assert_eq!(
		futures::executor::block_on(
			after.check_or_prompt_device(HostDevicePermissionRequest::Camera)
		)
		.unwrap(),
		PermissionAuthorizationStatus::Authorized,
	);
}

#[test]
fn a_prompt_failure_cannot_mask_a_stored_grant() {
	let storage = MemStorage::default();
	grant_stored(&storage, HostDevicePermissionRequest::Camera);

	// A failing prompt callback resolves to `NotDetermined`, which the
	// runtime reports as `granted: false`. A grant the product already
	// holds must never be reached through that path.
	let failing = FailingPrompt;
	let reset = ScriptedStatus::always(DevicePermissionStatus::NotDetermined);
	let service =
		PermissionsService::new(&storage, &failing, &PRODUCT).with_status_host(Some(&reset));
	assert_eq!(
		futures::executor::block_on(
			service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
		)
		.unwrap(),
		PermissionAuthorizationStatus::Authorized,
	);
}

#[test]
fn a_first_request_still_prompts_while_the_os_is_undetermined() {
	// The guard against re-prompting must not swallow the first ask, which
	// is the only one that establishes the product decision at all.
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![true], vec![]);
	let status = ScriptedStatus::always(DevicePermissionStatus::NotDetermined);
	let service =
		PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&status));

	assert_eq!(
		futures::executor::block_on(
			service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
		)
		.unwrap(),
		PermissionAuthorizationStatus::Authorized,
	);
	assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn an_os_reset_does_not_reprompt_a_stored_denial() {
	let storage = MemStorage::default();
	let seed = ScriptedPrompt::new(vec![false], vec![]);
	futures::executor::block_on(
		PermissionsService::new(&storage, &seed, &PRODUCT)
			.check_or_prompt_device(HostDevicePermissionRequest::Camera),
	)
	.unwrap();

	// The product-level "no" is still the user's answer. An OS that forgot
	// its own state is not a reason to put the question again.
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let status = ScriptedStatus::always(DevicePermissionStatus::NotDetermined);
	let service =
		PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&status));

	assert_eq!(
		futures::executor::block_on(
			service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
		)
		.unwrap(),
		PermissionAuthorizationStatus::Denied,
	);
	assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn a_failed_status_query_falls_back_to_the_stored_grant() {
	let storage = MemStorage::default();
	grant_stored(&storage, HostDevicePermissionRequest::Camera);

	// A dropped IPC is not a refusal. Reading it as one would let a flaky
	// channel revoke a capability the OS still allows.
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let status = ScriptedStatus::failing();
	let service =
		PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&status));

	assert_eq!(
		futures::executor::block_on(
			service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
		)
		.unwrap(),
		PermissionAuthorizationStatus::Authorized,
	);
	assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn an_os_denial_denies_before_any_prompt_and_persists_nothing() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let denied = ScriptedStatus::always(DevicePermissionStatus::Denied);
	let service =
		PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&denied));

	assert_eq!(
		futures::executor::block_on(
			service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
		)
		.unwrap(),
		PermissionAuthorizationStatus::Denied,
	);

	// Nothing was written, so the product question is still open: once the
	// OS allows it, the user gets asked rather than inheriting a denial
	// they never gave.
	let peek = PermissionsService::new(&storage, &prompt, &PRODUCT);
	assert_eq!(
		futures::executor::block_on(peek.peek_device(&HostDevicePermissionRequest::Camera))
			.unwrap(),
		PermissionAuthorizationStatus::NotDetermined,
	);
}

#[test]
fn os_status_is_read_for_the_capability_being_checked() {
	let storage = MemStorage::default();
	grant_stored(&storage, HostDevicePermissionRequest::Camera);
	grant_stored(&storage, HostDevicePermissionRequest::Microphone);

	// Only the camera is refused by the OS. A mix-up in which capability
	// reaches the status host would move the denial to the microphone.
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let status = ScriptedStatus::per_capability(
		vec![(HostDevicePermissionRequest::Camera, DevicePermissionStatus::Denied)],
		DevicePermissionStatus::Granted,
	);
	let service =
		PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&status));

	assert_eq!(
		futures::executor::block_on(
			service.check_or_prompt_device(HostDevicePermissionRequest::Camera)
		)
		.unwrap(),
		PermissionAuthorizationStatus::Denied,
	);
	assert_eq!(
		futures::executor::block_on(
			service.check_or_prompt_device(HostDevicePermissionRequest::Microphone)
		)
		.unwrap(),
		PermissionAuthorizationStatus::Authorized,
	);
	assert_eq!(
		status.asked(),
		vec![HostDevicePermissionRequest::Camera, HostDevicePermissionRequest::Microphone,],
	);
}

#[test]
fn a_host_without_the_capability_resolves_from_stored_state_alone() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![true], vec![]);
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(None);

	let first = futures::executor::block_on(
		service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
	)
	.unwrap();
	let second = futures::executor::block_on(
		service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
	)
	.unwrap();

	assert_eq!(first, PermissionAuthorizationStatus::Authorized);
	assert_eq!(second, PermissionAuthorizationStatus::Authorized);
	assert_eq!(prompt.device_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn os_device_status_does_not_reach_remote_permissions() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![true]);
	// Every device capability refused by the OS. A remote grant is a
	// TrUAPI-level decision with no OS gate behind it, so it must resolve
	// untouched.
	let status = ScriptedStatus::always(DevicePermissionStatus::Denied);
	let service =
		PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&status));

	assert_eq!(
		futures::executor::block_on(
			service.check_or_prompt_remote(remote_domains(&["example.com"]))
		)
		.unwrap(),
		PermissionAuthorizationStatus::Authorized,
	);
	assert!(status.asked().is_empty());
}

#[test]
fn a_peeked_grant_the_os_refuses_reads_as_denied() {
	// A settings screen and a request must not disagree. Reporting the
	// stored grant here sends the user hunting for a product toggle when
	// the OS is what refused.
	let storage = MemStorage::default();
	grant_stored(&storage, HostDevicePermissionRequest::Camera);
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let refusing = ScriptedStatus::always(DevicePermissionStatus::Denied);
	let service =
		PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&refusing));

	assert_eq!(
		futures::executor::block_on(service.peek_device(&HostDevicePermissionRequest::Camera))
			.unwrap(),
		PermissionAuthorizationStatus::Denied,
	);
}

#[test]
fn a_peek_under_an_os_refusal_leaves_the_stored_grant_intact() {
	let storage = MemStorage::default();
	grant_stored(&storage, HostDevicePermissionRequest::Camera);
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let refusing = ScriptedStatus::always(DevicePermissionStatus::Denied);
	futures::executor::block_on(
		PermissionsService::new(&storage, &prompt, &PRODUCT)
			.with_status_host(Some(&refusing))
			.peek_device(&HostDevicePermissionRequest::Camera),
	)
	.unwrap();

	// A read is not a decision, so the grant is still there once the user
	// restores the OS grant.
	let restored = ScriptedStatus::always(DevicePermissionStatus::Granted);
	assert_eq!(
		futures::executor::block_on(
			PermissionsService::new(&storage, &prompt, &PRODUCT)
				.with_status_host(Some(&restored))
				.peek_device(&HostDevicePermissionRequest::Camera),
		)
		.unwrap(),
		PermissionAuthorizationStatus::Authorized,
	);
}

#[test]
fn a_peeked_grant_survives_an_os_reset_and_a_failed_query() {
	let storage = MemStorage::default();
	grant_stored(&storage, HostDevicePermissionRequest::Camera);
	let prompt = ScriptedPrompt::new(vec![], vec![]);

	for status in
		[ScriptedStatus::always(DevicePermissionStatus::NotDetermined), ScriptedStatus::failing()]
	{
		assert_eq!(
			futures::executor::block_on(
				PermissionsService::new(&storage, &prompt, &PRODUCT)
					.with_status_host(Some(&status))
					.peek_device(&HostDevicePermissionRequest::Camera),
			)
			.unwrap(),
			PermissionAuthorizationStatus::Authorized,
			"only an outright refusal overrides the stored decision"
		);
	}
}

#[test]
fn a_status_read_of_a_remote_permission_never_reaches_the_os() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![true]);
	let refusing = ScriptedStatus::always(DevicePermissionStatus::Denied);
	let service =
		PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&refusing));
	futures::executor::block_on(service.check_or_prompt_remote(remote_domains(&["example.com"])))
		.unwrap();

	assert_eq!(
		futures::executor::block_on(service.authorization_status(
			&PermissionAuthorizationRequest::Remote(remote_domains(&["example.com"]))
		))
		.unwrap(),
		PermissionAuthorizationStatus::Authorized,
	);
	assert!(refusing.asked().is_empty());
}

#[test]
fn a_device_request_and_a_status_read_agree_once_the_os_refuses() {
	let storage = MemStorage::default();
	grant_stored(&storage, HostDevicePermissionRequest::Camera);
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let refusing = ScriptedStatus::always(DevicePermissionStatus::Denied);
	let service =
		PermissionsService::new(&storage, &prompt, &PRODUCT).with_status_host(Some(&refusing));

	let requested = futures::executor::block_on(
		service.check_or_prompt_device(HostDevicePermissionRequest::Camera),
	)
	.unwrap();
	let read = futures::executor::block_on(service.authorization_status(
		&PermissionAuthorizationRequest::Device(HostDevicePermissionRequest::Camera),
	))
	.unwrap();
	assert_eq!(
		(requested, read),
		(PermissionAuthorizationStatus::Denied, PermissionAuthorizationStatus::Denied,)
	);
}

/// A host can see and take back a signature approval through the
/// administration API. It reaches the key the runtime checks, and nothing
/// is stored, because the approval ends when the product closes.
#[test]
fn a_signature_approval_is_administered_in_memory_only() {
	let storage = MemStorage::default();
	let prompt = ScriptedPrompt::new(vec![], vec![]);
	let grants = Arc::new(TemporaryPermissions::default());
	let service = PermissionsService::new(&storage, &prompt, &PRODUCT)
		.with_temporary_permissions(Arc::clone(&grants));
	let request = PermissionAuthorizationRequest::CrossProductSignature {
		target_product_id: ProductId::for_test("dim2.dot"),
		kind: CrossProductSignatureKind::Payload,
	};
	let checked_by_runtime = cross_product_signature_key(
		&PRODUCT.product_id,
		&crate::platform::ProductId::for_test("dim2.dot"),
		CrossProductSignatureKind::Payload,
	);

	futures::executor::block_on(
		service.set_authorization_status(&request, PermissionAuthorizationStatus::Authorized),
	)
	.unwrap();
	assert_eq!(
		(
			futures::executor::block_on(service.authorization_status(&request)).unwrap(),
			grants.authorize(&checked_by_runtime, false),
		),
		(PermissionAuthorizationStatus::Authorized, true),
	);

	futures::executor::block_on(
		service.set_authorization_status(&request, PermissionAuthorizationStatus::NotDetermined),
	)
	.unwrap();
	assert_eq!(
		(
			futures::executor::block_on(service.authorization_status(&request)).unwrap(),
			grants.authorize(&checked_by_runtime, false),
			futures::executor::block_on(storage.inner.lock()).is_empty(),
		),
		(PermissionAuthorizationStatus::NotDetermined, false, true),
	);
}

#[test]
fn a_signature_approval_for_a_target_that_names_no_product_is_refused() {
	let encoded = (4u8, "example", CrossProductSignatureKind::Payload).encode();
	assert!(PermissionAuthorizationRequest::decode(&mut encoded.as_slice()).is_err());
}
