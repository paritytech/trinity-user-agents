//! Product-facing platform capability adapters.

use crate::platform::PermissionAuthorizationStatus;
use futures::StreamExt;
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
		locale::{HostLocaleSubscribeError, HostLocaleSubscribeItem, HostLocaleSubscribeRequest},
		notifications::{
			HostPushNotificationCancelError, HostPushNotificationCancelRequest,
			HostPushNotificationCancelResponse, HostPushNotificationError,
			HostPushNotificationRequest, HostPushNotificationResponse,
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

use crate::{
	host_internal::product_manifest::Granted,
	host_logic::{
		dotns::{NavigateDecision, parse_navigate},
		features::feature_supported,
	},
	runtime::{PERMISSION_DENIED_REASON, ProductRuntimeHost},
};

#[truapi::async_trait]
impl System for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "system.feature_supported"))]
	async fn feature_supported(
		&self,
		_cx: &CallContext,
		request: HostFeatureSupportedRequest,
	) -> Result<HostFeatureSupportedResponse, CallError<HostFeatureSupportedError>> {
		let version = request.version();
		let inner = request.into_latest();
		feature_supported(self.platform.as_ref(), inner)
			.await
			.map(|value| HostFeatureSupportedResponse::from_latest(value, version))
			.map_err(|err| CallError::Domain(HostFeatureSupportedError::from_latest(err, version)))
	}

	#[instrument(skip_all, fields(runtime.method = "system.host_info"))]
	async fn host_info(
		&self,
		_cx: &CallContext,
		request: HostInfoRequest,
	) -> Result<HostInfoResponse, CallError<HostInfoError>> {
		let version = request.version();
		let () = request.into_latest();
		let info = &self.services.host_info;
		Ok(HostInfoResponse::from_latest(
			latest::HostInfoResponse {
				platform: info.platform.clone(),
				name: info.name.clone(),
				version: info.version.clone().unwrap_or_default(),
			},
			version,
		))
	}

	#[instrument(skip_all, fields(runtime.method = "system.navigate_to"))]
	async fn navigate_to(
		&self,
		cx: &CallContext,
		request: HostNavigateToRequest,
	) -> Result<HostNavigateToResponse, CallError<HostNavigateToError>> {
		let version = request.version();
		let latest::HostNavigateToRequest { url } = request.into_latest();
		let resolved = match parse_navigate(&url) {
			NavigateDecision::Reject { reason } => {
				return Err(CallError::Domain(HostNavigateToError::from_latest(
					latest::HostNavigateToError::Unknown { reason },
					version,
				)));
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
					.permissions_service()
					.authorize_device(latest::HostDevicePermissionRequest::OpenUrl)
					.await
					.map_err(|error| CallError::HostFailure {
						reason: format!("permission storage failed: {error:?}"),
					})?;
				if status != PermissionAuthorizationStatus::Authorized {
					return Err(CallError::Domain(HostNavigateToError::from_latest(
						latest::HostNavigateToError::PermissionDenied,
						version,
					)));
				}
				url
			},
		};
		if let Some(reason) = cx.cancel().reason() {
			return Err(CallError::Domain(HostNavigateToError::from_latest(
				latest::HostNavigateToError::Unknown { reason: format!("navigation {reason}") },
				version,
			)));
		}
		self.platform
			.navigate_to(resolved)
			.await
			.map(|()| HostNavigateToResponse::from_latest((), version))
			.map_err(|err| CallError::Domain(HostNavigateToError::from_latest(err, version)))
	}

	#[instrument(skip_all, fields(runtime.method = "system.get_product_context"))]
	async fn get_product_context(
		&self,
		_cx: &CallContext,
		request: HostGetProductContextRequest,
	) -> Result<HostGetProductContextResponse, CallError<HostGetProductContextError>> {
		let version = request.version();
		let () = request.into_latest();
		Ok(HostGetProductContextResponse::from_latest(
			latest::HostGetProductContextResponse { product_id: self.product.product_id.clone() },
			version,
		))
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
		let service = self.permissions_service();
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
		let service = self.permissions_service();
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
		_cx: &CallContext,
		request: HostDevicePermissionRequest,
	) -> Result<HostDevicePermissionResponse, CallError<HostDevicePermissionError>> {
		let version = request.version();
		let inner = request.into_latest();
		let service = self.permissions_service();
		match service.check_or_prompt_device(inner).await {
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
		_cx: &CallContext,
		request: RemotePermissionRequest,
	) -> Result<RemotePermissionResponse, CallError<RemotePermissionError>> {
		let version = request.version();
		let inner = request.into_latest();
		let service = self.permissions_service();
		match service.check_or_prompt_remote(inner).await {
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
		let version = request.version();
		let latest::HostLocalStorageReadRequest { product, key } = request.into_latest();

		// One refusal for every reason the grant is not held: telling them apart
		// would make this call a probe for which products exist and which hold
		// data. A prompt is not the fallback either, since stored values are
		// opaque bytes nobody could inspect to approve.
		let owner = match product {
			Some(target) => {
				match self.cross_product_scope_target(&target, Granted::Storage).await {
					Some(owner) => owner,
					None => {
						return Err(CallError::Domain(HostLocalStorageReadError::from_latest(
							latest::HostLocalStorageReadError::AccessNotGranted,
							version,
						)));
					},
				}
			},
			None => self.product_id(),
		};

		self.platform
			.read(self.product_storage_key(&owner, key))
			.await
			.map(|value| {
				HostLocalStorageReadResponse::from_latest(
					latest::HostLocalStorageReadResponse { value },
					version,
				)
			})
			.map_err(|err| {
				CallError::Domain(HostLocalStorageReadError::from_latest(err.into(), version))
			})
	}

	#[instrument(skip_all, fields(runtime.method = "local_storage.write"))]
	async fn write(
		&self,
		_cx: &CallContext,
		request: HostLocalStorageWriteRequest,
	) -> Result<HostLocalStorageWriteResponse, CallError<HostLocalStorageWriteError>> {
		let version = request.version();
		let latest::HostLocalStorageWriteRequest { key, value } = request.into_latest();
		let storage_key = self.product_storage_key(self.product.product_id.as_str(), key);
		self.platform
			.write(storage_key, value)
			.await
			.map(|()| HostLocalStorageWriteResponse::from_latest((), version))
			.map_err(|err| CallError::Domain(HostLocalStorageWriteError::from_latest(err, version)))
	}

	#[instrument(skip_all, fields(runtime.method = "local_storage.clear"))]
	async fn clear(
		&self,
		_cx: &CallContext,
		request: HostLocalStorageClearRequest,
	) -> Result<HostLocalStorageClearResponse, CallError<HostLocalStorageClearError>> {
		let version = request.version();
		let latest::HostLocalStorageClearRequest { key } = request.into_latest();
		self.platform
			.clear(self.product_storage_key(self.product.product_id.as_str(), key))
			.await
			.map(|()| HostLocalStorageClearResponse::from_latest((), version))
			.map_err(|err| CallError::Domain(HostLocalStorageClearError::from_latest(err, version)))
	}

	#[instrument(skip_all, fields(runtime.method = "local_storage.subscribe"))]
	async fn subscribe(
		&self,
		_cx: &CallContext,
		request: HostLocalStorageSubscribeRequest,
	) -> Subscription<HostLocalStorageChangeItem, CallError<HostLocalStorageSubscribeError>> {
		let version = request.version();
		let latest::HostLocalStorageSubscribeRequest { key } = request.into_latest();
		// A write that left the bytes alone is not a change, and the
		// subscription is where that holds for every host: the core cannot
		// know whether one reports repeats, and withholding the write instead
		// would hide it from a host hanging quota or sync off it.
		let mut delivered: Option<Option<Vec<u8>>> = None;
		let stream = self
			.platform
			.subscribe_storage(self.product_storage_key(self.product.product_id.as_str(), key))
			.filter_map(move |item| {
				let next = match item {
					Ok(item) if delivered.as_ref() == Some(&item.value) => None,
					Ok(item) => {
						delivered = Some(item.value.clone());
						Some(Ok(HostLocalStorageChangeItem::from_latest(item, version)))
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
		let version = request.version();
		let latest::HostWorkerBeginOperationRequest { label } = request.into_latest();
		let response =
			self.begin_operation_with_host(label.unwrap_or_default())
				.await
				.map_err(|error| {
					CallError::Domain(HostWorkerBeginOperationError::from_latest(error, version))
				})?;
		self.hold_worker_for_operation(response.id);
		Ok(HostWorkerBeginOperationResponse::from_latest(response, version))
	}

	#[instrument(skip_all, fields(runtime.method = "worker.end_operation"))]
	async fn end_operation(
		&self,
		_cx: &CallContext,
		request: HostWorkerEndOperationRequest,
	) -> Result<HostWorkerEndOperationResponse, CallError<HostWorkerEndOperationError>> {
		let version = request.version();
		let latest::HostWorkerEndOperationRequest { id } = request.into_latest();
		let ended = self.platform.end_operation(&self.product, id).await;
		// The product has declared the operation over, so the core stops
		// counting it whatever the host made of the call. A host that dropped
		// the operation and still failed would otherwise leave demand standing
		// with nothing left able to end it, and a retry releases nothing.
		self.release_worker_for_operation(id);
		ended.map_err(|error| {
			CallError::Domain(HostWorkerEndOperationError::from_latest(error, version))
		})?;
		Ok(HostWorkerEndOperationResponse::from_latest((), version))
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
		let stream = self.platform.subscribe_theme().map(move |item| match item {
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
		let stream = self.platform.subscribe_locale().map(move |item| match item {
			Ok(item) => Ok(HostLocaleSubscribeItem::from_latest(item, version)),
			Err(error) => {
				warn!(reason = %error.reason, "locale platform stream failed");
				Err(CallError::HostFailure { reason: error.reason })
			},
		});
		Subscription::new(stream)
	}
}

// `Notifications` delegates to the platform so hosts can own scheduling and
// cancellation while the core preserves the typed TrUAPI wire shape.

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
		self.platform
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
		self.platform
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
}
