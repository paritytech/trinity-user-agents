//! Product-facing platform capability adapters.

use crate::platform::PermissionAuthorizationStatus;
use futures::StreamExt;
use parity_scale_codec::{Decode, Encode};
use tracing::{instrument, warn};
use truapi::api::{LocalStorage, Locale, Notifications, Permissions, System, Theme, Worker};
use truapi::versioned::IntoLatest;
use truapi::versioned::local_storage::{
    HostLocalStorageChangeItem, HostLocalStorageClearError, HostLocalStorageClearRequest,
    HostLocalStorageClearResponse, HostLocalStorageReadError, HostLocalStorageReadRequest,
    HostLocalStorageReadResponse, HostLocalStorageSubscribeError, HostLocalStorageSubscribeRequest,
    HostLocalStorageWriteError, HostLocalStorageWriteRequest, HostLocalStorageWriteResponse,
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
    HostNotificationReceiverStatusRequest, HostNotificationReceiverStatusResponse,
    HostNotificationReplaceReceiverRequest, HostNotificationReplaceReceiverResponse,
    HostNotificationDisableReceiverRequest, HostNotificationDisableReceiverResponse,
    HostNotificationRecordReceiptRequest, HostNotificationRecordReceiptResponse,
    HostNotificationReceiverEventsRequest, HostNotificationReceiverEventsResponse,
    HostNotificationAcknowledgeReceiverEventRequest, HostNotificationAcknowledgeReceiverEventResponse,
    HostNotificationReceivingError,
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

use crate::host_internal::product_manifest::Granted;
use crate::host_logic::dotns::{NavigateDecision, parse_navigate};
use crate::host_logic::features::feature_supported;
use crate::runtime::{PERMISSION_DENIED_REASON, ProductRuntimeHost};

#[truapi::async_trait]
impl System for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "system.feature_supported"))]
    async fn feature_supported(
        &self,
        _cx: &CallContext,
        request: HostFeatureSupportedRequest,
    ) -> Result<HostFeatureSupportedResponse, CallError<HostFeatureSupportedError>> {
        let HostFeatureSupportedRequest::V1(inner) = request;
        feature_supported(self.platform.as_ref(), inner)
            .await
            .map(HostFeatureSupportedResponse::V1)
            .map_err(|err| CallError::Domain(HostFeatureSupportedError::V1(err)))
    }

    #[instrument(skip_all, fields(runtime.method = "system.host_info"))]
    async fn host_info(
        &self,
        _cx: &CallContext,
        request: HostInfoRequest,
    ) -> Result<HostInfoResponse, CallError<HostInfoError>> {
        let HostInfoRequest::V1 = request;
        let info = &self.services.host_info;
        Ok(HostInfoResponse::V1(v01::HostInfo {
            platform: info.platform.clone(),
            name: info.name.clone(),
            version: info.version.clone().unwrap_or_default(),
        }))
    }

    #[instrument(skip_all, fields(runtime.method = "system.navigate_to"))]
    async fn navigate_to(
        &self,
        cx: &CallContext,
        request: HostNavigateToRequest,
    ) -> Result<HostNavigateToResponse, CallError<HostNavigateToError>> {
        let HostNavigateToRequest::V1(v01::HostNavigateToRequest { url }) = request;
        let resolved = match parse_navigate(&url) {
            NavigateDecision::Reject { reason } => {
                return Err(CallError::Domain(HostNavigateToError::V1(
                    v01::HostNavigateToError::Unknown { reason },
                )));
            }
            // dotNS, localhost and a host-handled Pocket target all resolve
            // back into the host's own product surface, which is already gated
            // by the product sandbox. None reaches an arbitrary internet host,
            // so none consumes a grant.
            NavigateDecision::DotName { canonical_url, .. }
            | NavigateDecision::Localhost { canonical_url, .. }
            | NavigateDecision::Pocket { canonical_url, .. } => canonical_url,
            NavigateDecision::External { url } => {
                let status = self
                    .permissions_service()
                    .authorize_device(v01::HostDevicePermissionRequest::OpenUrl)
                    .await
                    .map_err(|error| CallError::HostFailure {
                        reason: format!("permission storage failed: {error:?}"),
                    })?;
                if status != PermissionAuthorizationStatus::Authorized {
                    return Err(CallError::Domain(HostNavigateToError::V1(
                        v01::HostNavigateToError::PermissionDenied,
                    )));
                }
                url
            }
        };
        if let Some(reason) = cx.cancel().reason() {
            return Err(CallError::Domain(HostNavigateToError::V1(
                v01::HostNavigateToError::Unknown {
                    reason: format!("navigation {reason}"),
                },
            )));
        }
        self.platform
            .navigate_to(resolved)
            .await
            .map(|()| HostNavigateToResponse::V1)
            .map_err(|err| CallError::Domain(HostNavigateToError::V1(err)))
    }

    #[instrument(skip_all, fields(runtime.method = "system.get_product_context"))]
    async fn get_product_context(
        &self,
        _cx: &CallContext,
        _request: HostGetProductContextRequest,
    ) -> Result<HostGetProductContextResponse, CallError<HostGetProductContextError>> {
        Ok(HostGetProductContextResponse::V1(
            v01::HostGetProductContextResponse {
                product_id: self.product.product_id.clone(),
            },
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
        let HostDevicePermissionRequest::V1(inner) = request;
        let service = self.permissions_service();
        match service.authorize_device(inner).await {
            Ok(decision) => Ok(HostDevicePermissionResponse::V1(
                v01::HostDevicePermissionResponse {
                    granted: decision == PermissionAuthorizationStatus::Authorized,
                },
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
        let RemotePermissionRequest::V1(inner) = request;
        let service = self.permissions_service();
        match service.authorize_remote(inner).await {
            Ok(decision) => Ok(RemotePermissionResponse::V1(
                v01::RemotePermissionResponse {
                    granted: decision == PermissionAuthorizationStatus::Authorized,
                },
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
        let HostDevicePermissionRequest::V1(inner) = request;
        let service = self.permissions_service();
        match service.check_or_prompt_device(inner).await {
            Ok(decision) => Ok(HostDevicePermissionResponse::V1(
                v01::HostDevicePermissionResponse {
                    granted: decision == PermissionAuthorizationStatus::Authorized,
                },
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
        let RemotePermissionRequest::V1(inner) = request;
        let service = self.permissions_service();
        match service.check_or_prompt_remote(inner).await {
            Ok(decision) => Ok(RemotePermissionResponse::V1(
                v01::RemotePermissionResponse {
                    granted: decision == PermissionAuthorizationStatus::Authorized,
                },
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
        let v02::HostLocalStorageReadRequest { product, key } = request.into_latest();

        // One refusal for every reason the grant is not held: telling them apart
        // would make this call a probe for which products exist and which hold
        // data. A prompt is not the fallback either, since stored values are
        // opaque bytes nobody could inspect to approve.
        let owner = match product {
            Some(target) => {
                match self
                    .cross_product_scope_target(&target, Granted::Storage)
                    .await
                {
                    Some(owner) => owner,
                    None => {
                        return Err(CallError::Domain(HostLocalStorageReadError::V2(
                            v02::HostLocalStorageReadError::AccessNotGranted,
                        )));
                    }
                }
            }
            None => self.product_id(),
        };

        self.platform
            .read(self.product_storage_key(&owner, key))
            .await
            .map(|value| {
                HostLocalStorageReadResponse::V2(v01::HostLocalStorageReadResponse { value })
            })
            .map_err(|err| {
                CallError::Domain(HostLocalStorageReadError::V2(
                    HostLocalStorageReadError::V1(err).into_latest(),
                ))
            })
    }

    #[instrument(skip_all, fields(runtime.method = "local_storage.write"))]
    async fn write(
        &self,
        _cx: &CallContext,
        request: HostLocalStorageWriteRequest,
    ) -> Result<HostLocalStorageWriteResponse, CallError<HostLocalStorageWriteError>> {
        let HostLocalStorageWriteRequest::V1(v01::HostLocalStorageWriteRequest { key, value }) =
            request;
        let storage_key = self.product_storage_key(self.product.product_id.as_str(), key);
        self.platform
            .write(storage_key, value)
            .await
            .map(|()| HostLocalStorageWriteResponse::V1)
            .map_err(|err| CallError::Domain(HostLocalStorageWriteError::V1(err)))
    }

    #[instrument(skip_all, fields(runtime.method = "local_storage.clear"))]
    async fn clear(
        &self,
        _cx: &CallContext,
        request: HostLocalStorageClearRequest,
    ) -> Result<HostLocalStorageClearResponse, CallError<HostLocalStorageClearError>> {
        let HostLocalStorageClearRequest::V1(v01::HostLocalStorageClearRequest { key }) = request;
        self.platform
            .clear(self.product_storage_key(self.product.product_id.as_str(), key))
            .await
            .map(|()| HostLocalStorageClearResponse::V1)
            .map_err(|err| CallError::Domain(HostLocalStorageClearError::V1(err)))
    }

    #[instrument(skip_all, fields(runtime.method = "local_storage.subscribe"))]
    async fn subscribe(
        &self,
        _cx: &CallContext,
        request: HostLocalStorageSubscribeRequest,
    ) -> Subscription<HostLocalStorageChangeItem, CallError<HostLocalStorageSubscribeError>> {
        let HostLocalStorageSubscribeRequest::V1(v01::HostLocalStorageSubscribeRequest { key }) =
            request;
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
                        Some(Ok(HostLocalStorageChangeItem::V1(item)))
                    }
                    Err(error) => {
                        warn!(
                            reason = %error.reason,
                            "local storage subscription platform stream failed"
                        );
                        Some(Err(CallError::HostFailure {
                            reason: error.reason,
                        }))
                    }
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
        let HostWorkerBeginOperationRequest::V1(v01::HostWorkerBeginOperationRequest { label }) =
            request;
        let response = self
            .begin_operation_with_host(label.unwrap_or_default())
            .await
            .map_err(|error| CallError::Domain(HostWorkerBeginOperationError::V1(error)))?;
        self.hold_worker_for_operation(response.id);
        Ok(HostWorkerBeginOperationResponse::V1(response))
    }

    #[instrument(skip_all, fields(runtime.method = "worker.end_operation"))]
    async fn end_operation(
        &self,
        _cx: &CallContext,
        request: HostWorkerEndOperationRequest,
    ) -> Result<HostWorkerEndOperationResponse, CallError<HostWorkerEndOperationError>> {
        let HostWorkerEndOperationRequest::V1(v01::HostWorkerEndOperationRequest { id }) = request;
        let ended = self.platform.end_operation(&self.product, id).await;
        // The product has declared the operation over, so the core stops
        // counting it whatever the host made of the call. A host that dropped
        // the operation and still failed would otherwise leave demand standing
        // with nothing left able to end it, and a retry releases nothing.
        self.release_worker_for_operation(id);
        ended.map_err(|error| CallError::Domain(HostWorkerEndOperationError::V1(error)))?;
        Ok(HostWorkerEndOperationResponse::V1)
    }
}

#[truapi::async_trait]
impl Theme for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "theme.subscribe"))]
    async fn subscribe(
        &self,
        _cx: &CallContext,
        _request: HostThemeSubscribeRequest,
    ) -> Subscription<HostThemeSubscribeItem, CallError<HostThemeSubscribeError>> {
        let stream = self.platform.subscribe_theme().map(|item| match item {
            Ok(item) => Ok(HostThemeSubscribeItem::V1(item)),
            Err(error) => {
                warn!(reason = %error.reason, "theme platform stream failed");
                Err(CallError::HostFailure {
                    reason: error.reason,
                })
            }
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
            .ok_or(CallError::Domain(HostNotificationReceivingError::V1(
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
        let HostPushNotificationRequest::V1(inner) = request;
        let status = self
            .permissions_service()
            .authorize_device(v01::HostDevicePermissionRequest::Notifications)
            .await
            .map_err(|err| CallError::HostFailure {
                reason: format!("permission storage failed: {err:?}"),
            })?;
        if status != PermissionAuthorizationStatus::Authorized {
            return Err(CallError::Domain(HostPushNotificationError::V1(
                v01::HostPushNotificationError::Unknown {
                    reason: PERMISSION_DENIED_REASON.to_string(),
                },
            )));
        }
        if let Some(reason) = cx.cancel().reason() {
            return Err(CallError::Domain(HostPushNotificationError::V1(
                v01::HostPushNotificationError::Unknown {
                    reason: format!("notification {reason}"),
                },
            )));
        }
        self.platform
            .push_notification(inner)
            .await
            .map(HostPushNotificationResponse::V1)
            .map_err(|err| {
                CallError::Domain(HostPushNotificationError::V1(
                    v01::HostPushNotificationError::Unknown { reason: err.reason },
                ))
            })
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

    async fn receiver_status(
        &self,
        _cx: &CallContext,
        _request: HostNotificationReceiverStatusRequest,
    ) -> Result<HostNotificationReceiverStatusResponse, CallError<HostNotificationReceivingError>> {
        if let Some(status) = self.forwarded_receiving(2, Vec::new()).await? {
            return Ok(HostNotificationReceiverStatusResponse::V1(status));
        }
        let authority = match self.receiving_execution_authority().await {
            Ok(authority) => authority,
            Err(CallError::Domain(HostNotificationReceivingError::V1(
                crate::latest::HostNotificationReceivingError::Unsupported,
            ))) => return Ok(HostNotificationReceiverStatusResponse::V1(
                crate::latest::HostNotificationReceiverStatus {
                    supported: false, os_permission: false, consent: false, enabled: false,
                    revision: 0, sync_pending: false, transport_ready: false,
                },
            )),
            Err(error) => return Err(error),
        };
        self.services.receiving.for_execution(authority).status().await
            .map(HostNotificationReceiverStatusResponse::V1)
            .map_err(|error| CallError::Domain(HostNotificationReceivingError::V1(error)))
    }

    async fn replace_receiver(
        &self,
        cx: &CallContext,
        request: HostNotificationReplaceReceiverRequest,
    ) -> Result<HostNotificationReplaceReceiverResponse, CallError<HostNotificationReceivingError>> {
        let HostNotificationReplaceReceiverRequest::V1(request) = request;
        let status = self.permissions_service()
            .authorize_device(v01::HostDevicePermissionRequest::Notifications).await
            .map_err(|error| CallError::Domain(HostNotificationReceivingError::V1(
                crate::latest::HostNotificationReceivingError::Storage {
                    reason: format!("permission storage failed: {error:?}"),
                },
            )))?;
        if status != PermissionAuthorizationStatus::Authorized {
            return Err(CallError::Domain(HostNotificationReceivingError::V1(
                crate::latest::HostNotificationReceivingError::PermissionDenied,
            )));
        }
        if let Some(reason) = cx.cancel().reason() {
            return Err(CallError::Domain(HostNotificationReceivingError::V1(
                crate::latest::HostNotificationReceivingError::InvalidRequest {
                    reason: format!("receiving enrollment {reason}"),
                },
            )));
        }
        if let Some(status) = self.forwarded_receiving(3, request.encode()).await? {
            return Ok(HostNotificationReplaceReceiverResponse::V1(status));
        }
        let authority = self.receiving_execution_authority().await?;
        self.services.receiving.for_execution(authority)
            .replace(request.expected_revision, request.watches).await
            .map(HostNotificationReplaceReceiverResponse::V1)
            .map_err(|error| CallError::Domain(HostNotificationReceivingError::V1(error)))
    }

    async fn disable_receiver(
        &self,
        _cx: &CallContext,
        request: HostNotificationDisableReceiverRequest,
    ) -> Result<HostNotificationDisableReceiverResponse, CallError<HostNotificationReceivingError>> {
        let HostNotificationDisableReceiverRequest::V1(request) = request;
        if let Some(status) = self.forwarded_receiving(4, request.encode()).await? {
            return Ok(HostNotificationDisableReceiverResponse::V1(status));
        }
        let authority = self.receiving_execution_authority().await?;
        self.services.receiving.for_execution(authority).disable(request.expected_revision).await
            .map(HostNotificationDisableReceiverResponse::V1)
            .map_err(|error| CallError::Domain(HostNotificationReceivingError::V1(error)))
    }

    async fn record_receipt(
        &self,
        _cx: &CallContext,
        request: HostNotificationRecordReceiptRequest,
    ) -> Result<HostNotificationRecordReceiptResponse, CallError<HostNotificationReceivingError>> {
        let HostNotificationRecordReceiptRequest::V1(request) = request;
        if let Some(outcome) = self.forwarded_receiving(5, request.encode()).await? {
            return Ok(HostNotificationRecordReceiptResponse::V1(outcome));
        }
        let authority = self.receiving_execution_authority().await?;
        self.services.receiving.for_execution(authority).receipt(
            request.revision, request.watch_id, request.event_id, request.kind,
        ).await
            .map(HostNotificationRecordReceiptResponse::V1)
            .map_err(|error| CallError::Domain(HostNotificationReceivingError::V1(error)))
    }

    async fn receiver_events(
        &self,
        _cx: &CallContext,
        request: HostNotificationReceiverEventsRequest,
    ) -> Result<HostNotificationReceiverEventsResponse, CallError<HostNotificationReceivingError>> {
        let HostNotificationReceiverEventsRequest::V1(request) = request;
        if let Some(events) = self.forwarded_receiving(6, request.encode()).await? {
            return Ok(HostNotificationReceiverEventsResponse::V1(events));
        }
        let authority = self.receiving_execution_authority().await?;
        self.services.receiving.for_execution(authority).events(request.after_sequence).await
            .map(HostNotificationReceiverEventsResponse::V1)
            .map_err(|error| CallError::Domain(HostNotificationReceivingError::V1(error)))
    }

    async fn acknowledge_receiver_event(
        &self,
        _cx: &CallContext,
        request: HostNotificationAcknowledgeReceiverEventRequest,
    ) -> Result<HostNotificationAcknowledgeReceiverEventResponse, CallError<HostNotificationReceivingError>> {
        let HostNotificationAcknowledgeReceiverEventRequest::V1(request) = request;
        if let Some(()) = self.forwarded_receiving(7, request.encode()).await? {
            return Ok(HostNotificationAcknowledgeReceiverEventResponse::V1);
        }
        let authority = self.receiving_execution_authority().await?;
        self.services.receiving.for_execution(authority).acknowledge(request.sequence).await
            .map(|()| HostNotificationAcknowledgeReceiverEventResponse::V1)
            .map_err(|error| CallError::Domain(HostNotificationReceivingError::V1(error)))
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
