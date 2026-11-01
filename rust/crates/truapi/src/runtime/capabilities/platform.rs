//! Product-facing platform capability adapters.

use std::sync::atomic::{AtomicU64, Ordering};

use super::from_latest_error;
use crate::{
	host_internal::{permissions::PermissionAuthorizationSnapshot, product_manifest::Granted},
	host_logic::{
		dotns::{NavigateDecision, parse_navigate},
		features::feature_supported,
	},
	platform::{
		PermissionAuthorizationRequest, PermissionAuthorizationStatus, PermissionDecision,
		ProductId,
	},
	runtime::{PERMISSION_DENIED_REASON, ProductRuntimeHost, versioning::version},
};
use futures::StreamExt;
use parity_scale_codec::{Decode, Encode};
use tracing::{instrument, warn};
use truapi::{
	CallContext, CallError, Subscription,
	api::{LocalStorage, Locale, Notifications, Permissions, System, Theme, Worker},
	latest,
	versioned::{
		FromLatest, IntoLatest, Versioned,
		local_storage::{
			HostLocalStorageChangeItem, HostLocalStorageClearError, HostLocalStorageClearRequest,
			HostLocalStorageClearResponse, HostLocalStorageReadError, HostLocalStorageReadRequest,
			HostLocalStorageReadResponse, HostLocalStorageSubscribeError,
			HostLocalStorageSubscribeRequest, HostLocalStorageWriteError,
			HostLocalStorageWriteRequest, HostLocalStorageWriteResponse,
		},
		locale::{
			HostLocaleLocalizeTimestampsError, HostLocaleLocalizeTimestampsRequest,
			HostLocaleLocalizeTimestampsResponse, HostLocaleSubscribeError,
			HostLocaleSubscribeItem, HostLocaleSubscribeRequest,
		},
		notifications::{
			HostNotificationAcknowledgeReceiverEventRequest,
			HostNotificationAcknowledgeReceiverEventResponse,
			HostNotificationDisableReceiverRequest, HostNotificationDisableReceiverResponse,
			HostNotificationReceiverEventsRequest, HostNotificationReceiverEventsResponse,
			HostNotificationReceiverStatusRequest, HostNotificationReceiverStatusResponse,
			HostNotificationReceivingError, HostNotificationRecordReceiptRequest,
			HostNotificationRecordReceiptResponse, HostNotificationReplaceReceiverRequest,
			HostNotificationReplaceReceiverResponse, HostPushNotificationCancelError,
			HostPushNotificationCancelRequest, HostPushNotificationCancelResponse,
			HostPushNotificationError, HostPushNotificationRequest, HostPushNotificationResponse,
			NotificationActivationAcknowledgeError, NotificationActivationAcknowledgeRequest,
			NotificationActivationAcknowledgeResponse, NotificationActivationEventsError,
			NotificationActivationEventsRequest, NotificationActivationEventsResponse,
		},
		permissions::{
			HostDevicePermissionError, HostDevicePermissionRequest, HostDevicePermissionResponse,
			RemotePermissionError, RemotePermissionRequest, RemotePermissionResponse,
		},
		system::{
			HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
			HostGetProductContextError, HostGetProductContextRequest,
			HostGetProductContextResponse, HostInfoError, HostInfoRequest, HostInfoResponse,
			HostNavigateToError, HostNavigateToRequest, HostNavigateToResponse,
		},
		theme::{HostThemeSubscribeError, HostThemeSubscribeItem, HostThemeSubscribeRequest},
		worker::{
			HostWorkerBeginOperationError, HostWorkerBeginOperationRequest,
			HostWorkerBeginOperationResponse, HostWorkerEndOperationError,
			HostWorkerEndOperationRequest, HostWorkerEndOperationResponse,
		},
	},
};

impl ProductRuntimeHost {
	/// Resolve a host settings slot without prompting, advertising or starting Media.
	pub async fn calling_permission_authorization_request(
		&self,
	) -> Result<PermissionAuthorizationRequest, latest::GenericError> {
		let (_, identity) = crate::runtime::media_identity::resolve_identity(
			&self.connection.services,
			self.accounts.media_authority().as_ref(),
			&self.connection.product,
			&CallContext::default(),
		)
		.await
		.map_err(|_| latest::GenericError { reason: "Calling identity is unavailable".into() })?;
		Ok(PermissionAuthorizationRequest::Calling {
			network: identity.network,
			account: identity.account,
		})
	}

	async fn commit_permission_answer(
		&self,
		product_id: &str,
		request: &PermissionAuthorizationRequest,
		snapshot: &PermissionAuthorizationSnapshot,
		decision: PermissionDecision,
		revision: &AtomicU64,
		current: &(impl Fn() -> bool + Sync),
	) -> Result<PermissionAuthorizationStatus, latest::GenericError> {
		let _guard = self.connection.services.media_permission_gate.lock().await;
		if !current() {
			return Ok(PermissionAuthorizationStatus::NotDetermined);
		}
		let service = self.connection.permissions_service();
		// Recheck the live OS overlay, without rebasing the captured generation.
		if service.authorization_status(request).await? !=
			PermissionAuthorizationStatus::NotDetermined
		{
			return Ok(PermissionAuthorizationStatus::NotDetermined);
		}
		if !current() {
			return Ok(PermissionAuthorizationStatus::NotDetermined);
		}
		// No await between these final guards and CAS dispatch. Cancellation
		// after dispatch does not roll back an accepted product decision.
		self.connection.services.advance_media_permission_revision()?;
		revision.store(self.connection.services.media_permission_revision(), Ordering::Release);
		let status = service
			.record_permission_decision_if_unchanged(snapshot, decision, false)
			.await?;
		if status == PermissionAuthorizationStatus::NotDetermined {
			return Ok(status);
		}
		if status != PermissionAuthorizationStatus::Authorized {
			self.connection.services.notify_media_permission_revoked(product_id, request);
		}
		Ok(status)
	}
}

#[truapi::async_trait]
impl System for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "system.feature_supported"))]
	async fn feature_supported(
		&self,
		_cx: &CallContext,
		request: HostFeatureSupportedRequest,
	) -> Result<HostFeatureSupportedResponse, CallError<HostFeatureSupportedError>> {
		version(request)
			.exec(|inner| async move {
				feature_supported(self.connection.platform.as_ref(), inner)
					.await
					.map_err(CallError::Domain)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "system.host_info"))]
	async fn host_info(
		&self,
		_cx: &CallContext,
		request: HostInfoRequest,
	) -> Result<HostInfoResponse, CallError<HostInfoError>> {
		version(request)
			.exec(|()| async move {
				let info = &self.connection.services.host_info;
				Ok(latest::HostInfoResponse {
					platform: info.platform.clone(),
					name: info.name.clone(),
					version: info.version.clone().unwrap_or_default(),
					truapi_version: env!("CARGO_PKG_VERSION").to_string(),
				})
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "system.navigate_to"))]
	async fn navigate_to(
		&self,
		cx: &CallContext,
		request: HostNavigateToRequest,
	) -> Result<HostNavigateToResponse, CallError<HostNavigateToError>> {
		version(request)
			.exec(|latest::HostNavigateToRequest { url }| async move {
				let resolved = match parse_navigate(&url) {
					NavigateDecision::Reject { reason } => {
						return Err(CallError::Domain(latest::HostNavigateToError::Unknown {
							reason,
						}));
					},
					// dotNS, localhost and a host-handled Pocket target all resolve
					// back into the host's own product surface, which is already gated
					// by the product sandbox. None reaches an arbitrary internet host,
					// so none consumes a grant.
					NavigateDecision::DotName { canonical_url, .. } |
					NavigateDecision::Localhost { canonical_url, .. } |
					NavigateDecision::Pocket { canonical_url, .. } |
					NavigateDecision::PocketCollection { canonical_url, .. } => canonical_url,
					NavigateDecision::External { url } => {
						let status = self
							.connection
							.permissions_service()
							.authorize_device(latest::HostDevicePermissionRequest::OpenUrl)
							.await
							.map_err(|error| CallError::HostFailure {
								reason: format!("permission storage failed: {error:?}"),
							})?;
						if status != PermissionAuthorizationStatus::Authorized {
							return Err(CallError::Domain(
								latest::HostNavigateToError::PermissionDenied,
							));
						}
						url
					},
				};
				if let Some(reason) = cx.cancel().reason() {
					return Err(CallError::Domain(latest::HostNavigateToError::Unknown {
						reason: format!("navigation {reason}"),
					}));
				}
				self.connection.platform.navigate_to(resolved).await.map_err(CallError::Domain)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "system.get_product_context"))]
	async fn get_product_context(
		&self,
		_cx: &CallContext,
		request: HostGetProductContextRequest,
	) -> Result<HostGetProductContextResponse, CallError<HostGetProductContextError>> {
		version(request)
			.exec(|()| async move {
				Ok(latest::HostGetProductContextResponse {
					product_id: self.connection.product.product_id.clone(),
				})
			})
			.await
	}
}

#[truapi::async_trait]
impl Permissions for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "permissions.authorize_device_permission"))]
	async fn authorize_device_permission(
		&self,
		_cx: &CallContext,
		request: HostDevicePermissionRequest,
	) -> Result<HostDevicePermissionResponse, CallError<HostDevicePermissionError>> {
		let version = request.version();
		let inner = request.into_latest();
		let service = self.connection.permissions_service();
		match service.authorize_device(inner).await {
			Ok(decision) => Ok(HostDevicePermissionResponse::from_latest(
				latest::HostDevicePermissionResponse {
					granted: decision == PermissionAuthorizationStatus::Authorized,
				},
				version,
			)),
			Err(err) => Err(CallError::HostFailure {
				reason: format!("permission storage failed: {err:?}"),
			}),
		}
	}

	#[instrument(skip_all, fields(runtime.method = "permissions.authorize_remote_permission"))]
	async fn authorize_remote_permission(
		&self,
		_cx: &CallContext,
		request: RemotePermissionRequest,
	) -> Result<RemotePermissionResponse, CallError<RemotePermissionError>> {
		let version = request.version();
		let inner = request.into_latest();
		let service = self.connection.permissions_service();
		match service.authorize_remote(inner).await {
			Ok(decision) => Ok(RemotePermissionResponse::from_latest(
				latest::RemotePermissionResponse {
					granted: decision == PermissionAuthorizationStatus::Authorized,
				},
				version,
			)),
			Err(err) => Err(CallError::HostFailure {
				reason: format!("permission storage failed: {err:?}"),
			}),
		}
	}

	#[instrument(skip_all, fields(runtime.method = "permissions.request_device_permission"))]
	async fn request_device_permission(
		&self,
		cx: &CallContext,
		request: HostDevicePermissionRequest,
	) -> Result<HostDevicePermissionResponse, CallError<HostDevicePermissionError>> {
		let version = request.version();
		let inner = request.into_latest();
		let product_id = self.connection.product_id().as_str();
		let service = self.connection.permissions_service();
		let request = PermissionAuthorizationRequest::Device(inner);
		let session = self.accounts.current_session();
		let revision = {
			let _guard = self.connection.services.media_permission_gate.lock().await;
			AtomicU64::new(self.connection.services.media_permission_revision())
		};
		let current = || {
			self.connection.services.media_permission_revision() == revision.load(Ordering::Acquire) &&
				self.accounts.current_session() == session &&
				!cx.cancel().is_cancelled()
		};
		let decision = service
			.check_or_prompt_device_fenced(inner, |snapshot, decision| {
				let current = &current;
				let revision = &revision;
				let request = &request;
				let product_id = &product_id;
				async move {
					self.commit_permission_answer(
						product_id, request, &snapshot, decision, revision, current,
					)
					.await
				}
			})
			.await;
		let _guard = self.connection.services.media_permission_gate.lock().await;
		if !current() {
			return Err(CallError::Denied);
		}
		let decision = match decision {
			Ok(_) => service.authorization_status(&request).await,
			Err(error) => Err(error),
		};
		if !current() {
			return Err(CallError::Denied);
		}
		match decision {
			Ok(decision) => Ok(HostDevicePermissionResponse::from_latest(
				latest::HostDevicePermissionResponse {
					granted: decision == PermissionAuthorizationStatus::Authorized,
				},
				version,
			)),
			Err(err) => Err(CallError::HostFailure {
				reason: format!("permission storage failed: {err:?}"),
			}),
		}
	}

	#[instrument(skip_all, fields(runtime.method = "permissions.request_remote_permission"))]
	async fn request_remote_permission(
		&self,
		cx: &CallContext,
		request: RemotePermissionRequest,
	) -> Result<RemotePermissionResponse, CallError<RemotePermissionError>> {
		let version = request.version();
		let inner = request.into_latest();
		let product_id = self.connection.product_id().as_str();
		let service = self.connection.permissions_service();
		let decision = if matches!(inner.permission, latest::RemotePermission::Calling) {
			let (session, identity) = crate::runtime::media_identity::resolve_identity(
				&self.connection.services,
				self.accounts.media_authority().as_ref(),
				&self.connection.product,
				cx,
			)
			.await
			.map_err(|_| CallError::Denied)?;
			let request = PermissionAuthorizationRequest::Calling {
				network: identity.network,
				account: identity.account,
			};
			let revision = {
				let _guard = self.connection.services.media_permission_gate.lock().await;
				AtomicU64::new(self.connection.services.media_permission_revision())
			};
			let current = || {
				self.connection.services.media_permission_revision() ==
					revision.load(Ordering::Acquire) &&
					self.accounts.current_session().as_ref() == Some(&session) &&
					!cx.cancel().is_cancelled()
			};
			let decision = service
				.check_or_prompt_calling(identity.network, identity.account, |snapshot, status| {
					let request = &request;
					let current = &current;
					let revision = &revision;
					let product_id = &product_id;
					async move {
						let decision = match status {
							PermissionAuthorizationStatus::Authorized => {
								PermissionDecision::AllowAlways
							},
							PermissionAuthorizationStatus::Denied => PermissionDecision::Deny,
							PermissionAuthorizationStatus::NotDetermined => return Ok(status),
						};
						self.commit_permission_answer(
							product_id, request, &snapshot, decision, revision, current,
						)
						.await
					}
				})
				.await;
			let _guard = self.connection.services.media_permission_gate.lock().await;
			if !current() {
				return Err(CallError::Denied);
			}
			let decision = match decision {
				Ok(_) => service.authorization_status(&request).await,
				Err(error) => Err(error),
			};
			if !current() {
				return Err(CallError::Denied);
			}
			decision
		} else {
			service.check_or_prompt_remote(inner).await
		};
		match decision {
			Ok(decision) => Ok(RemotePermissionResponse::from_latest(
				latest::RemotePermissionResponse {
					granted: decision == PermissionAuthorizationStatus::Authorized,
				},
				version,
			)),
			Err(err) => Err(CallError::HostFailure {
				reason: format!("permission storage failed: {err:?}"),
			}),
		}
	}
}

#[truapi::async_trait]
impl LocalStorage for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "local_storage.read"))]
	async fn read(
		&self,
		_cx: &CallContext,
		request: HostLocalStorageReadRequest,
	) -> Result<HostLocalStorageReadResponse, CallError<HostLocalStorageReadError>> {
		version(request)
			.exec(|latest::HostLocalStorageReadRequest { product, key }| async move {
				// One refusal for every reason the grant is not held: telling them apart
				// would make this call a probe for which products exist and which hold
				// data. A prompt is not the fallback either, since stored values are
				// opaque bytes nobody could inspect to approve.
				let access_not_granted =
					|| CallError::Domain(latest::HostLocalStorageReadError::AccessNotGranted);
				let owner = match product {
					Some(target) => {
						let target =
							target.parse::<ProductId>().map_err(|_| access_not_granted())?;
						if !self
							.connection
							.holds_cross_product_scope(&target, Granted::Storage)
							.await
						{
							return Err(access_not_granted());
						}
						target
					},
					None => self.connection.product_id().clone(),
				};

				self.connection
					.platform
					.read(self.connection.product_storage_key(&owner, key))
					.await
					.map(|value| latest::HostLocalStorageReadResponse { value })
					.map_err(|err| CallError::Domain(err.into()))
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "local_storage.write"))]
	async fn write(
		&self,
		_cx: &CallContext,
		request: HostLocalStorageWriteRequest,
	) -> Result<HostLocalStorageWriteResponse, CallError<HostLocalStorageWriteError>> {
		version(request)
			.exec(|latest::HostLocalStorageWriteRequest { key, value }| async move {
				let storage_key =
					self.connection.product_storage_key(self.connection.product_id(), key);
				self.connection
					.platform
					.write(storage_key, value)
					.await
					.map_err(CallError::Domain)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "local_storage.clear"))]
	async fn clear(
		&self,
		_cx: &CallContext,
		request: HostLocalStorageClearRequest,
	) -> Result<HostLocalStorageClearResponse, CallError<HostLocalStorageClearError>> {
		version(request)
			.exec(|latest::HostLocalStorageClearRequest { key }| async move {
				self.connection
					.platform
					.clear(self.connection.product_storage_key(self.connection.product_id(), key))
					.await
					.map_err(CallError::Domain)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "local_storage.subscribe"))]
	async fn subscribe(
		&self,
		_cx: &CallContext,
		request: HostLocalStorageSubscribeRequest,
	) -> Subscription<HostLocalStorageChangeItem, CallError<HostLocalStorageSubscribeError>> {
		version(request)
			.exec(|latest::HostLocalStorageSubscribeRequest { key }| async move {
				// A write that left the bytes alone is not a change, and the
				// subscription is where that holds for every host: the core cannot
				// know whether one reports repeats, and withholding the write instead
				// would hide it from a host hanging quota or sync off it.
				let mut delivered: Option<Option<Vec<u8>>> = None;
				let stream = self
					.connection
					.platform
					.subscribe_storage(
						self.connection.product_storage_key(self.connection.product_id(), key),
					)
					.filter_map(move |item| {
						let next = match item {
							Ok(item) if delivered.as_ref() == Some(&item.value) => None,
							Ok(item) => {
								delivered = Some(item.value.clone());
								Some(Ok(item))
							},
							Err(error) => {
								warn!(
									reason = %error.reason,
									"local storage subscription platform stream failed"
								);
								Some(Err(CallError::HostFailure { reason: error.reason }))
							},
						};
						futures::future::ready(next)
					});
				Subscription::new(stream)
			})
			.await
	}
}

#[truapi::async_trait]
impl Worker for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "worker.begin_operation"))]
	async fn begin_operation(
		&self,
		_cx: &CallContext,
		request: HostWorkerBeginOperationRequest,
	) -> Result<HostWorkerBeginOperationResponse, CallError<HostWorkerBeginOperationError>> {
		version(request)
			.exec(|latest::HostWorkerBeginOperationRequest { label }| async move {
				let response = self
					.connection
					.begin_operation_with_host(label.unwrap_or_default())
					.await
					.map_err(CallError::Domain)?;
				self.connection.hold_worker_for_operation(response.id);
				Ok(response)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "worker.end_operation"))]
	async fn end_operation(
		&self,
		_cx: &CallContext,
		request: HostWorkerEndOperationRequest,
	) -> Result<HostWorkerEndOperationResponse, CallError<HostWorkerEndOperationError>> {
		version(request)
			.exec(|latest::HostWorkerEndOperationRequest { id }| async move {
				let ended =
					self.connection.platform.end_operation(&self.connection.product, id).await;
				// The product has declared the operation over, so the core stops
				// counting it whatever the host made of the call. A host that dropped
				// the operation and still failed would otherwise leave demand standing
				// with nothing left able to end it, and a retry releases nothing.
				self.connection.release_worker_for_operation(id);
				ended.map_err(CallError::Domain)
			})
			.await
	}
}

#[truapi::async_trait]
impl Theme for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "theme.subscribe"))]
	async fn subscribe(
		&self,
		_cx: &CallContext,
		request: HostThemeSubscribeRequest,
	) -> Subscription<HostThemeSubscribeItem, CallError<HostThemeSubscribeError>> {
		let version = request.version();
		let () = request.into_latest();
		let stream = self.connection.platform.subscribe_theme().map(move |item| match item {
			Ok(item) => Ok(HostThemeSubscribeItem::from_latest(item, version)),
			Err(error) => {
				warn!(reason = %error.reason, "theme platform stream failed");
				Err(CallError::HostFailure { reason: error.reason })
			},
		});
		Subscription::new(stream)
	}
}

#[truapi::async_trait]
impl Locale for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "locale.subscribe"))]
	async fn subscribe(
		&self,
		_cx: &CallContext,
		request: HostLocaleSubscribeRequest,
	) -> Subscription<HostLocaleSubscribeItem, CallError<HostLocaleSubscribeError>> {
		let version = request.version();
		let () = request.into_latest();
		let stream = self.connection.platform.subscribe_locale().map(move |item| match item {
			Ok(item) => Ok(HostLocaleSubscribeItem::from_latest(item, version)),
			Err(error) => {
				warn!(reason = %error.reason, "locale platform stream failed");
				Err(CallError::HostFailure { reason: error.reason })
			},
		});
		Subscription::new(stream)
	}

	#[instrument(skip_all, fields(runtime.method = "locale.localize_timestamps"))]
	async fn localize_timestamps(
		&self,
		_cx: &CallContext,
		request: HostLocaleLocalizeTimestampsRequest,
	) -> Result<HostLocaleLocalizeTimestampsResponse, CallError<HostLocaleLocalizeTimestampsError>>
	{
		version(request)
			.exec(|request| async move {
				if request.timestamps_ms.len() > 128 ||
					request
						.timestamps_ms
						.iter()
						.any(|timestamp| *timestamp > 253_402_300_799_999) ||
					request.language_tag.is_empty() ||
					request.time_zone.is_empty()
				{
					return Err(CallError::Domain(latest::GenericError {
						reason: "Invalid local time conversion request".into(),
					}));
				}
				let count = request.timestamps_ms.len();
				let response = self
					.connection
					.platform
					.localize_timestamps(request)
					.await
					.map_err(CallError::Domain)?;
				if response.timestamps.len() != count {
					return Err(CallError::HostFailure {
						reason: "Host returned an incomplete local time conversion".into(),
					});
				}
				Ok(response)
			})
			.await
	}
}

// Scheduling belongs to the platform; receiving belongs to its sole resident
// engine or explicitly forwarded external owner, never to a product lifetime.

impl ProductRuntimeHost {
	async fn forwarded_receiving<R: Decode>(
		&self,
		action: u8,
		payload: Vec<u8>,
	) -> Result<Option<R>, CallError<latest::HostNotificationReceivingError>> {
		let Some(bytes) = self
			.connection
			.platform
			.receiver_command(self.connection.product_id().to_string(), action, payload)
			.await
			.map_err(|error| CallError::HostFailure { reason: error.reason })?
		else {
			return Ok(None);
		};
		let mut input = bytes.as_slice();
		let result = Result::<R, crate::latest::HostNotificationReceivingError>::decode(&mut input)
			.map_err(|_| CallError::HostFailure {
				reason: "invalid receiving owner response".into(),
			})?;
		if !input.is_empty() {
			return Err(CallError::HostFailure {
				reason: "trailing receiving owner response bytes".into(),
			});
		}
		result.map(Some).map_err(CallError::Domain)
	}

	async fn receiving_execution_authority(
		&self,
	) -> Result<
		crate::platform::ReceivingAuthority,
		CallError<latest::HostNotificationReceivingError>,
	> {
		let authority = self
			.connection
			.platform
			.receiver_authority(self.connection.product_id().as_str())
			.await
			.map_err(|error| CallError::HostFailure { reason: error.reason })?
			.ok_or(CallError::Domain(latest::HostNotificationReceivingError::Unsupported))?;
		if authority.product_id != self.connection.product_id().as_str() {
			return Err(CallError::Domain(
				latest::HostNotificationReceivingError::PermissionDenied,
			));
		}
		Ok(authority)
	}
}

#[truapi::async_trait]
impl Notifications for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "notifications.send_push_notification"))]
	async fn send_push_notification(
		&self,
		cx: &CallContext,
		request: HostPushNotificationRequest,
	) -> Result<HostPushNotificationResponse, CallError<HostPushNotificationError>> {
		let version = request.version();
		let inner = request.into_latest();
		let status = self
			.connection
			.permissions_service()
			.authorize_device(latest::HostDevicePermissionRequest::Notifications)
			.await
			.map_err(|err| CallError::HostFailure {
				reason: format!("permission storage failed: {err:?}"),
			})?;
		if status != PermissionAuthorizationStatus::Authorized {
			return Err(CallError::Domain(HostPushNotificationError::from_latest(
				latest::HostPushNotificationError::Unknown {
					reason: PERMISSION_DENIED_REASON.to_string(),
				},
				version,
			)));
		}
		if let Some(reason) = cx.cancel().reason() {
			return Err(CallError::Domain(HostPushNotificationError::from_latest(
				latest::HostPushNotificationError::Unknown {
					reason: format!("notification {reason}"),
				},
				version,
			)));
		}
		self.connection
			.platform
			.push_notification(inner)
			.await
			.map(|value| HostPushNotificationResponse::from_latest(value, version))
			.map_err(|err| {
				CallError::Domain(HostPushNotificationError::from_latest(
					latest::HostPushNotificationError::Unknown { reason: err.reason },
					version,
				))
			})
	}

	#[instrument(skip_all, fields(runtime.method = "notifications.cancel_push_notification"))]
	async fn cancel_push_notification(
		&self,
		_cx: &CallContext,
		request: HostPushNotificationCancelRequest,
	) -> Result<HostPushNotificationCancelResponse, CallError<HostPushNotificationCancelError>> {
		let version = request.version();
		let latest::HostPushNotificationCancelRequest { id } = request.into_latest();
		self.connection
			.platform
			.cancel_notification(id)
			.await
			.map(|()| HostPushNotificationCancelResponse::from_latest((), version))
			.map_err(|err| {
				CallError::Domain(HostPushNotificationCancelError::from_latest(
					latest::GenericError { reason: err.reason },
					version,
				))
			})
	}

	async fn receiver_status(
		&self,
		_cx: &CallContext,
		request: HostNotificationReceiverStatusRequest,
	) -> Result<HostNotificationReceiverStatusResponse, CallError<HostNotificationReceivingError>>
	{
		let version = request.version();
		let () = request.into_latest();
		let result = async {
			if let Some(status) = self.forwarded_receiving(2, Vec::new()).await? {
				return Ok(status);
			}
			let authority = match self.receiving_execution_authority().await {
				Ok(authority) => authority,
				Err(CallError::Domain(latest::HostNotificationReceivingError::Unsupported)) => {
					return Ok(latest::HostNotificationReceiverStatus {
						supported: false,
						os_permission: false,
						consent: false,
						enabled: false,
						revision: 0,
						sync_pending: false,
						transport_ready: false,
					});
				},
				Err(error) => return Err(error),
			};
			self.connection
				.services
				.receiving
				.for_execution(authority)
				.status()
				.await
				.map_err(CallError::Domain)
		}
		.await;
		result
			.map(|response| HostNotificationReceiverStatusResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostNotificationReceivingError>(error, version))
	}

	async fn replace_receiver(
		&self,
		cx: &CallContext,
		request: HostNotificationReplaceReceiverRequest,
	) -> Result<HostNotificationReplaceReceiverResponse, CallError<HostNotificationReceivingError>>
	{
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			let status = self
				.connection
				.permissions_service()
				.authorize_device(latest::HostDevicePermissionRequest::Notifications)
				.await
				.map_err(|error| {
					CallError::Domain(latest::HostNotificationReceivingError::Storage {
						reason: format!("permission storage failed: {error:?}"),
					})
				})?;
			if status != PermissionAuthorizationStatus::Authorized {
				return Err(CallError::Domain(
					latest::HostNotificationReceivingError::PermissionDenied,
				));
			}
			if let Some(reason) = cx.cancel().reason() {
				return Err(CallError::Domain(
					latest::HostNotificationReceivingError::InvalidRequest {
						reason: format!("receiving enrollment {reason}"),
					},
				));
			}
			if let Some(status) = self.forwarded_receiving(3, request.encode()).await? {
				return Ok(status);
			}
			let authority = self.receiving_execution_authority().await?;
			self.connection
				.services
				.receiving
				.for_execution(authority)
				.replace(request.expected_revision, request.watches)
				.await
				.map_err(CallError::Domain)
		}
		.await;
		result
			.map(|response| HostNotificationReplaceReceiverResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostNotificationReceivingError>(error, version))
	}

	async fn disable_receiver(
		&self,
		_cx: &CallContext,
		request: HostNotificationDisableReceiverRequest,
	) -> Result<HostNotificationDisableReceiverResponse, CallError<HostNotificationReceivingError>>
	{
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			if let Some(status) = self.forwarded_receiving(4, request.encode()).await? {
				return Ok(status);
			}
			let authority = self.receiving_execution_authority().await?;
			self.connection
				.services
				.receiving
				.for_execution(authority)
				.disable(request.expected_revision)
				.await
				.map_err(CallError::Domain)
		}
		.await;
		result
			.map(|response| HostNotificationDisableReceiverResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostNotificationReceivingError>(error, version))
	}

	async fn record_receipt(
		&self,
		_cx: &CallContext,
		request: HostNotificationRecordReceiptRequest,
	) -> Result<HostNotificationRecordReceiptResponse, CallError<HostNotificationReceivingError>> {
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			if let Some(outcome) = self.forwarded_receiving(5, request.encode()).await? {
				return Ok(outcome);
			}
			let authority = self.receiving_execution_authority().await?;
			self.connection
				.services
				.receiving
				.for_execution(authority)
				.receipt(request.revision, request.watch_id, request.event_id, request.kind)
				.await
				.map_err(CallError::Domain)
		}
		.await;
		result
			.map(|response| HostNotificationRecordReceiptResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostNotificationReceivingError>(error, version))
	}

	async fn receiver_events(
		&self,
		_cx: &CallContext,
		request: HostNotificationReceiverEventsRequest,
	) -> Result<HostNotificationReceiverEventsResponse, CallError<HostNotificationReceivingError>>
	{
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			if let Some(events) = self.forwarded_receiving(6, request.encode()).await? {
				return Ok(events);
			}
			let authority = self.receiving_execution_authority().await?;
			self.connection
				.services
				.receiving
				.for_execution(authority)
				.events(request.after_sequence)
				.await
				.map_err(CallError::Domain)
		}
		.await;
		result
			.map(|response| HostNotificationReceiverEventsResponse::from_latest(response, version))
			.map_err(|error| from_latest_error::<HostNotificationReceivingError>(error, version))
	}

	async fn acknowledge_receiver_event(
		&self,
		_cx: &CallContext,
		request: HostNotificationAcknowledgeReceiverEventRequest,
	) -> Result<
		HostNotificationAcknowledgeReceiverEventResponse,
		CallError<HostNotificationReceivingError>,
	> {
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			if let Some(()) = self.forwarded_receiving(7, request.encode()).await? {
				return Ok(());
			}
			let authority = self.receiving_execution_authority().await?;
			self.connection
				.services
				.receiving
				.for_execution(authority)
				.acknowledge(request.sequence)
				.await
				.map_err(CallError::Domain)
		}
		.await;
		result
			.map(|response| {
				HostNotificationAcknowledgeReceiverEventResponse::from_latest(response, version)
			})
			.map_err(|error| from_latest_error::<HostNotificationReceivingError>(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "notifications.activation_events"))]
	async fn activation_events(
		&self,
		_cx: &CallContext,
		request: NotificationActivationEventsRequest,
	) -> Result<NotificationActivationEventsResponse, CallError<NotificationActivationEventsError>>
	{
		let version = request.version();
		let () = request.into_latest();
		let events = self.connection.platform.activation_events().await.map_err(|error| {
			from_latest_error::<NotificationActivationEventsError>(
				CallError::Domain(error),
				version,
			)
		})?;
		if events.events.len() > 32 {
			return Err(CallError::HostFailure {
				reason: "notification activation batch exceeds 32 events".to_string(),
			});
		}
		Ok(NotificationActivationEventsResponse::from_latest(events, version))
	}

	#[instrument(skip_all, fields(runtime.method = "notifications.acknowledge_activation"))]
	async fn acknowledge_activation(
		&self,
		_cx: &CallContext,
		request: NotificationActivationAcknowledgeRequest,
	) -> Result<
		NotificationActivationAcknowledgeResponse,
		CallError<NotificationActivationAcknowledgeError>,
	> {
		let version = request.version();
		let request = request.into_latest();
		self.connection
			.platform
			.acknowledge_activation(request)
			.await
			.map(|()| NotificationActivationAcknowledgeResponse::from_latest((), version))
			.map_err(|error| {
				from_latest_error::<NotificationActivationAcknowledgeError>(
					CallError::Domain(error),
					version,
				)
			})
	}
}
