//! Product-facing platform capability adapters.

use crate::platform::{PermissionAuthorizationStatus, ProductId};
use futures::StreamExt;
use parity_scale_codec::{Decode, Encode};
use tracing::{instrument, warn};
use truapi::{
	CallContext, CallError, Subscription,
	api::{LocalStorage, Locale, Notifications, Permissions, System, Theme, Worker},
	latest,
	versioned::{
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
use truapi::versioned::locale::{
    HostLocaleSubscribeError, HostLocaleSubscribeItem, HostLocaleSubscribeRequest,
    HostLocaleLocalizeTimestampsError, HostLocaleLocalizeTimestampsRequest,
    HostLocaleLocalizeTimestampsResponse,
};
use truapi::versioned::notifications::{
    HostPushNotificationCancelError, HostPushNotificationCancelRequest,
    HostPushNotificationCancelResponse, HostPushNotificationError, HostPushNotificationRequest,
    HostPushNotificationResponse,
    NotificationActivationAcknowledgeError, NotificationActivationAcknowledgeRequest,
    NotificationActivationAcknowledgeResponse, NotificationActivationEventsError,
    NotificationActivationEventsRequest, NotificationActivationEventsResponse,
};
use truapi::versioned::permissions::{
    HostDevicePermissionError, HostDevicePermissionRequest, HostDevicePermissionResponse,
    RemotePermissionError, RemotePermissionRequest, RemotePermissionResponse,
};
use truapi::versioned::system::{
    HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
    HostGetProductContextError, HostGetProductContextRequest, HostGetProductContextResponse,
    HostInfoError, HostInfoRequest, HostInfoResponse, HostNavigateToError, HostNavigateToRequest,
    HostNavigateToResponse,
};
use truapi::versioned::theme::{
    HostThemeSubscribeError, HostThemeSubscribeItem, HostThemeSubscribeRequest,
};
use truapi::versioned::worker::{
    HostWorkerBeginOperationError, HostWorkerBeginOperationRequest,
    HostWorkerBeginOperationResponse, HostWorkerEndOperationError, HostWorkerEndOperationRequest,
    HostWorkerEndOperationResponse,
};
use truapi::{CallContext, CallError, Subscription, v01, v02};

use crate::{
	host_internal::product_manifest::Granted,
	host_logic::{
		dotns::{NavigateDecision, parse_navigate},
		features::feature_supported,
	},
	runtime::{PERMISSION_DENIED_REASON, ProductRuntimeHost, versioning::version},
};

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
		version(request)
			.exec(|inner| async move {
				let service = self.connection.permissions_service();
				match service.authorize_device(inner).await {
					Ok(decision) => Ok(latest::HostDevicePermissionResponse {
						granted: decision == PermissionAuthorizationStatus::Authorized,
					}),
					Err(err) => Err(CallError::HostFailure {
						reason: format!("permission storage failed: {err:?}"),
					}),
				}
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "permissions.authorize_remote_permission"))]
	async fn authorize_remote_permission(
		&self,
		_cx: &CallContext,
		request: RemotePermissionRequest,
	) -> Result<RemotePermissionResponse, CallError<RemotePermissionError>> {
		version(request)
			.exec(|inner| async move {
				let service = self.connection.permissions_service();
				match service.authorize_remote(inner).await {
					Ok(decision) => Ok(latest::RemotePermissionResponse {
						granted: decision == PermissionAuthorizationStatus::Authorized,
					}),
					Err(err) => Err(CallError::HostFailure {
						reason: format!("permission storage failed: {err:?}"),
					}),
				}
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "permissions.request_device_permission"))]
	async fn request_device_permission(
		&self,
		_cx: &CallContext,
		request: HostDevicePermissionRequest,
	) -> Result<HostDevicePermissionResponse, CallError<HostDevicePermissionError>> {
		version(request)
			.exec(|inner| async move {
				let service = self.connection.permissions_service();
				match service.check_or_prompt_device(inner).await {
					Ok(decision) => Ok(latest::HostDevicePermissionResponse {
						granted: decision == PermissionAuthorizationStatus::Authorized,
					}),
					Err(err) => Err(CallError::HostFailure {
						reason: format!("permission storage failed: {err:?}"),
					}),
				}
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "permissions.request_remote_permission"))]
	async fn request_remote_permission(
		&self,
		_cx: &CallContext,
		request: RemotePermissionRequest,
	) -> Result<RemotePermissionResponse, CallError<RemotePermissionError>> {
		version(request)
			.exec(|inner| async move {
				let service = self.connection.permissions_service();
				match service.check_or_prompt_remote(inner).await {
					Ok(decision) => Ok(latest::RemotePermissionResponse {
						granted: decision == PermissionAuthorizationStatus::Authorized,
					}),
					Err(err) => Err(CallError::HostFailure {
						reason: format!("permission storage failed: {err:?}"),
					}),
				}
			})
			.await
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
		version(request)
			.exec(|()| async move {
				let stream = self.connection.platform.subscribe_theme().map(|item| {
					item.map_err(|error| {
						warn!(reason = %error.reason, "theme platform stream failed");
						CallError::HostFailure { reason: error.reason }
					})
				});
				Subscription::new(stream)
			})
			.await
	}
}

#[truapi::async_trait]
impl Locale for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "locale.subscribe"))]
    async fn subscribe(
        &self,
        _cx: &CallContext,
        _request: HostLocaleSubscribeRequest,
    ) -> Subscription<HostLocaleSubscribeItem, CallError<HostLocaleSubscribeError>> {
        let stream = self.platform.subscribe_locale().map(|item| match item {
            Ok(item) => Ok(HostLocaleSubscribeItem::V2(item)),
            Err(error) => {
                warn!(reason = %error.reason, "locale platform stream failed");
                Err(CallError::HostFailure {
                    reason: error.reason,
                })
            }
        });
        Subscription::new(stream)
    }

    #[instrument(skip_all, fields(runtime.method = "locale.localize_timestamps"))]
    async fn localize_timestamps(
        &self,
        _cx: &CallContext,
        request: HostLocaleLocalizeTimestampsRequest,
    ) -> Result<HostLocaleLocalizeTimestampsResponse, CallError<HostLocaleLocalizeTimestampsError>> {
        let request = request.into_latest();
        if request.timestamps_ms.len() > 128
            || request.timestamps_ms.iter().any(|timestamp| *timestamp > 253_402_300_799_999)
            || request.language_tag.is_empty()
            || request.time_zone.is_empty()
        {
            return Err(CallError::Domain(HostLocaleLocalizeTimestampsError::V1(
                truapi::latest::GenericError { reason: "Invalid local time conversion request".into() },
            )));
        }
        let count = request.timestamps_ms.len();
        let response = self.platform.localize_timestamps(request).await.map_err(|error| {
            CallError::Domain(HostLocaleLocalizeTimestampsError::V1(error))
        })?;
        if response.timestamps.len() != count {
            return Err(CallError::HostFailure {
                reason: "Host returned an incomplete local time conversion".into(),
            });
        }
        Ok(HostLocaleLocalizeTimestampsResponse::V1(response))
    }
}

// Scheduling belongs to the platform; receiving belongs to its sole resident
// engine or explicitly forwarded external owner, never to a product lifetime.

impl ProductRuntimeHost {
    async fn forwarded_receiving<R: Decode>(
        &self, action: u8, payload: Vec<u8>,
    ) -> Result<Option<R>, CallError<HostNotificationReceivingError>> {
        let Some(bytes) = self.platform.receiver_command(
            self.product.product_id.clone(), action, payload,
        ).await.map_err(|error| CallError::HostFailure { reason: error.reason })? else {
            return Ok(None);
        };
        let mut input = bytes.as_slice();
        let result = Result::<R, crate::latest::HostNotificationReceivingError>::decode(&mut input)
            .map_err(|_| CallError::HostFailure { reason: "invalid receiving owner response".into() })?;
        if !input.is_empty() {
            return Err(CallError::HostFailure { reason: "trailing receiving owner response bytes".into() });
        }
        result.map(Some).map_err(|error| CallError::Domain(HostNotificationReceivingError::V1(error)))
    }

    async fn receiving_execution_authority(
        &self,
    ) -> Result<crate::platform::ReceivingAuthority, CallError<HostNotificationReceivingError>> {
        let authority = self.platform.receiver_authority(&self.product.product_id).await
            .map_err(|error| CallError::HostFailure { reason: error.reason })?
            .ok_or_else(|| CallError::Domain(HostNotificationReceivingError::V1(
                crate::latest::HostNotificationReceivingError::Unsupported,
            )))?;
        if authority.product_id != self.product.product_id {
            return Err(CallError::Domain(HostNotificationReceivingError::V1(
                crate::latest::HostNotificationReceivingError::PermissionDenied,
            )));
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
		version(request)
			.exec(|inner| async move {
				let status = self
					.connection
					.permissions_service()
					.authorize_device(latest::HostDevicePermissionRequest::Notifications)
					.await
					.map_err(|err| CallError::HostFailure {
						reason: format!("permission storage failed: {err:?}"),
					})?;
				if status != PermissionAuthorizationStatus::Authorized {
					return Err(CallError::Domain(latest::HostPushNotificationError::Unknown {
						reason: PERMISSION_DENIED_REASON.to_string(),
					}));
				}
				if let Some(reason) = cx.cancel().reason() {
					return Err(CallError::Domain(latest::HostPushNotificationError::Unknown {
						reason: format!("notification {reason}"),
					}));
				}
				self.connection.platform.push_notification(inner).await.map_err(|err| {
					CallError::Domain(latest::HostPushNotificationError::Unknown {
						reason: err.reason,
					})
				})
			})
			.await
	}

    #[instrument(skip_all, fields(runtime.method = "notifications.cancel_push_notification"))]
    async fn cancel_push_notification(
        &self,
        _cx: &CallContext,
        request: HostPushNotificationCancelRequest,
    ) -> Result<HostPushNotificationCancelResponse, CallError<HostPushNotificationCancelError>>
    {
        let HostPushNotificationCancelRequest::V1(v01::HostPushNotificationCancelRequest { id }) =
            request;
        self.platform
            .cancel_notification(id)
            .await
            .map(|()| HostPushNotificationCancelResponse::V1)
            .map_err(|err| {
                CallError::Domain(HostPushNotificationCancelError::V1(v01::GenericError {
                    reason: err.reason,
                }))
            })
    }

    #[instrument(skip_all, fields(runtime.method = "notifications.activation_events"))]
    async fn activation_events(
        &self,
        _cx: &CallContext,
        request: NotificationActivationEventsRequest,
    ) -> Result<NotificationActivationEventsResponse, CallError<NotificationActivationEventsError>> {
        let NotificationActivationEventsRequest::V1 = request;
        let events = self.platform.activation_events().await.map_err(|err| {
            CallError::Domain(NotificationActivationEventsError::V1(err))
        })?;
        if events.events.len() > 32 {
            return Err(CallError::HostFailure {
                reason: "notification activation batch exceeds 32 events".to_string(),
            });
        }
        Ok(NotificationActivationEventsResponse::V1(events))
    }

    #[instrument(skip_all, fields(runtime.method = "notifications.acknowledge_activation"))]
    async fn acknowledge_activation(
        &self,
        _cx: &CallContext,
        request: NotificationActivationAcknowledgeRequest,
    ) -> Result<NotificationActivationAcknowledgeResponse, CallError<NotificationActivationAcknowledgeError>> {
        let NotificationActivationAcknowledgeRequest::V1(
            v01::NotificationActivationAcknowledgeRequest { sequence },
        ) = request;
        self.platform
            .acknowledge_activation(v01::NotificationActivationAcknowledgeRequest { sequence })
            .await
            .map(|()| NotificationActivationAcknowledgeResponse::V1)
            .map_err(|err| CallError::Domain(NotificationActivationAcknowledgeError::V1(err)))
    }
}
