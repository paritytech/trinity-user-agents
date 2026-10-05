//! Product-facing platform capability adapters.

use crate::platform::PermissionAuthorizationStatus;
use futures::StreamExt;
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
};
use truapi::versioned::notifications::{
    HostPushNotificationCancelError, HostPushNotificationCancelRequest,
    HostPushNotificationCancelResponse, HostPushNotificationError, HostPushNotificationRequest,
    HostPushNotificationResponse,
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
impl<H: crate::runtime::AccountHolder + 'static> System for ProductRuntimeHost<H> {
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
impl<H: crate::runtime::AccountHolder + 'static> Permissions for ProductRuntimeHost<H> {
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
impl<H: crate::runtime::AccountHolder + 'static> LocalStorage for ProductRuntimeHost<H> {
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
impl<H: crate::runtime::AccountHolder + 'static> Worker for ProductRuntimeHost<H> {
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
impl<H: crate::runtime::AccountHolder + 'static> Theme for ProductRuntimeHost<H> {
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
impl<H: crate::runtime::AccountHolder + 'static> Locale for ProductRuntimeHost<H> {
    #[instrument(skip_all, fields(runtime.method = "locale.subscribe"))]
    async fn subscribe(
        &self,
        _cx: &CallContext,
        _request: HostLocaleSubscribeRequest,
    ) -> Subscription<HostLocaleSubscribeItem, CallError<HostLocaleSubscribeError>> {
        let stream = self.platform.subscribe_locale().map(|item| match item {
            Ok(item) => Ok(HostLocaleSubscribeItem::V1(item)),
            Err(error) => {
                warn!(reason = %error.reason, "locale platform stream failed");
                Err(CallError::HostFailure {
                    reason: error.reason,
                })
            }
        });
        Subscription::new(stream)
    }
}

// `Notifications` delegates to the platform so hosts can own scheduling and
// cancellation while the core preserves the typed TrUAPI wire shape.

#[truapi::async_trait]
impl<H: crate::runtime::AccountHolder + 'static> Notifications for ProductRuntimeHost<H> {
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
            .map_err(|error| CallError::Domain(HostPushNotificationError::V1(error)))
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
}
