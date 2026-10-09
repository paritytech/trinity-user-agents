//! Wallet-free receiving owner for a host-origin service worker.

use std::sync::Arc;
use js_sys::{Function, Uint8Array};
use parity_scale_codec::{Decode, Encode};
use send_wrapper::SendWrapper;
use wasm_bindgen::prelude::*;
use crate::latest::{self, HostNotificationReceivingError as Error};
use crate::platform::ReceivingAuthority;
use crate::runtime::receiving::{ReceivingBackend, ReceivingService};
use super::{get_function, get_optional_function, invoke_optional_bytes_return, invoke_bool, invoke_unit, generic, receiving_error_to_js};

struct JsReceivingBackend {
    authority: SendWrapper<Function>,
    consent: SendWrapper<Function>,
    changed: SendWrapper<Function>,
    load: SendWrapper<Function>,
    save: SendWrapper<Function>,
}

#[crate::platform::async_trait]
impl ReceivingBackend for JsReceivingBackend {
    async fn receiver_authority(&self, product: &str) -> Result<Option<ReceivingAuthority>, latest::GenericError> {
        let bytes = invoke_optional_bytes_return(
            &self.authority, vec![JsValue::from_str(product)],
            "receiverAuthority must return SCALE ReceivingAuthority or undefined",
        ).await.map_err(generic)?;
        bytes.map(|bytes| decode_exact(&bytes).map_err(generic)).transpose()
    }

    async fn receiver_consent(&self, authority: ReceivingAuthority, watches: Vec<latest::ReceivingWatch>) -> Result<bool, latest::GenericError> {
        invoke_bool(&self.consent, vec![
            Uint8Array::from(authority.encode().as_slice()).into(),
            Uint8Array::from(watches.encode().as_slice()).into(),
        ]).await.map_err(generic)
    }

    async fn receiver_changed(&self) -> Result<(), latest::GenericError> {
        invoke_unit(&self.changed, Vec::new()).await.map_err(generic)
    }

    async fn load(&self) -> Result<Option<Vec<u8>>, latest::GenericError> {
        invoke_optional_bytes_return(&self.load, Vec::new(), "readReceivingState must return bytes or undefined")
            .await.map_err(generic)
    }

    async fn save(&self, bytes: Vec<u8>) -> Result<(), latest::GenericError> {
        invoke_unit(&self.save, vec![Uint8Array::from(bytes.as_slice()).into()]).await.map_err(generic)
    }
}

fn decode_exact<T: Decode>(bytes: &[u8]) -> Result<T, String> {
    let mut input = bytes;
    let value = T::decode(&mut input).map_err(|_| "invalid receiving SCALE payload".to_string())?;
    if !input.is_empty() {
        return Err("trailing receiving SCALE payload bytes".into());
    }
    Ok(value)
}

/// Standalone receiver with one durable writer and no wallet or product execution.
/// The host must serialize ownership across service-worker replacement and bind
/// command product IDs to trusted execution sessions, never message-body claims.
#[wasm_bindgen]
pub struct WasmNotificationReceiver {
    service: Arc<ReceivingService>,
}

#[wasm_bindgen]
impl WasmNotificationReceiver {
    /// Construct from raw receiverAuthority/receiverConsent/receiverChanged,
    /// readReceivingState and writeReceivingState callbacks. Persistence callbacks
    /// are required; absent authority advertises unsupported, never enrollment.
    #[wasm_bindgen(constructor)]
    pub fn new(callbacks: JsValue) -> Result<WasmNotificationReceiver, JsValue> {
        let backend = JsReceivingBackend {
            authority: SendWrapper::new(get_optional_function(&callbacks, "receiverAuthority")?
                .unwrap_or_else(super::absent_optional_callback)),
            consent: SendWrapper::new(get_optional_function(&callbacks, "receiverConsent")?
                .unwrap_or_else(|| super::missing_callback("receiverConsent"))),
            changed: SendWrapper::new(get_optional_function(&callbacks, "receiverChanged")?
                .unwrap_or_else(|| super::missing_callback("receiverChanged"))),
            load: SendWrapper::new(get_function(&callbacks, "readReceivingState")?),
            save: SendWrapper::new(get_function(&callbacks, "writeReceivingState")?),
        };
        let spawner: crate::subscription::Spawner = Arc::new(|future| wasm_bindgen_futures::spawn_local(future));
        Ok(Self { service: Arc::new(ReceivingService::from_backend(Arc::new(backend), spawner)) })
    }

    /// Execute actions 2..7 under the immutable authority captured by the trusted
    /// execution channel. Authority is SCALE ReceivingAuthority, never page input.
    /// Request has no version tag; response is SCALE
    /// `Result<latest response, HostNotificationReceivingError>`. Action 3 requires
    /// the forwarding runtime's ordinary Notifications permission authorization.
    #[wasm_bindgen(js_name = commandForExecution)]
    pub async fn command_for_execution(&self, authority: Vec<u8>, action: u8, payload: Vec<u8>) -> Vec<u8> {
        let result = self.command_inner(&authority, action, &payload).await;
        match result {
            Ok(encoded) => encoded,
            Err(error) => Result::<(), Error>::Err(error).encode(),
        }
    }

    /// All local registrations, including synchronized ones, as SCALE Vec.
    #[wasm_bindgen(js_name = receivingPending)]
    pub async fn receiving_pending(&self) -> Result<Vec<u8>, JsValue> {
        self.service.pending().await.map(|value| value.encode()).map_err(receiving_error_to_js)
    }

    /// Acknowledge only the revision actually synchronized by transport.
    #[wasm_bindgen(js_name = receivingSynchronized)]
    pub async fn receiving_synchronized(&self, product_id: String, revision: u64) -> Result<bool, JsValue> {
        self.service.synchronized(&product_id, revision).await.map_err(receiving_error_to_js)
    }

    /// Authenticate observed source metadata and carrier; returns SCALE `Vec<ReceivingEvent>`.
    #[wasm_bindgen(js_name = receivingIngest)]
    pub async fn receiving_ingest(&self, product_id: String, revision: u64, watch_id: String,
        actual_genesis: String, actual_channel: String, actual_topics: Vec<String>, frame: Vec<u8>,
    ) -> Result<Vec<u8>, JsValue> {
        self.service.ingest(&product_id, revision, watch_id, actual_genesis, actual_channel, actual_topics, frame)
            .await.map(|value| value.encode()).map_err(receiving_error_to_js)
    }

    /// Authenticate a SCALE statement including source metadata; returns SCALE `Vec<ReceivingEvent>`.
    #[wasm_bindgen(js_name = receivingIngestStatement)]
    pub async fn receiving_ingest_statement(&self, product_id: String, revision: u64, watch_id: String,
        actual_genesis: String, statement: Vec<u8>,
    ) -> Result<Vec<u8>, JsValue> {
        self.service.ingest_statement(&product_id, revision, watch_id, actual_genesis, statement)
            .await.map(|value| value.encode()).map_err(receiving_error_to_js)
    }

    /// Revalidate and reserve display after foreground grace; returns SCALE `Option<ReceivingEvent>`.
    #[wasm_bindgen(js_name = receivingPrepareDisplay)]
    pub async fn receiving_prepare_display(&self, product_id: String, revision: u64, event_id: String) -> Result<Vec<u8>, JsValue> {
        self.service.prepare_display(&product_id, revision, event_id).await.map(|value| value.encode()).map_err(receiving_error_to_js)
    }

    /// Confirm actual visible display, not ingestion or transport acknowledgement.
    #[wasm_bindgen(js_name = receivingConfirmDisplay)]
    pub async fn receiving_confirm_display(&self, product_id: String, revision: u64, event_id: String) -> Result<(), JsValue> {
        self.service.confirm_display(&product_id, revision, event_id).await.map_err(receiving_error_to_js)
    }

    /// Clear a reservation only after explicit display failure, not an unknown outcome.
    #[wasm_bindgen(js_name = receivingCancelDisplay)]
    pub async fn receiving_cancel_display(&self, product_id: String, revision: u64, event_id: String) -> Result<(), JsValue> {
        self.service.cancel_display(&product_id, revision, event_id).await.map_err(receiving_error_to_js)
    }

    /// Read-only click validation before loading the verified product; sequence is zero.
    #[wasm_bindgen(js_name = receivingValidateActivation)]
    pub async fn receiving_validate_activation(&self, product_id: String, revision: u64, event_id: String) -> Result<Vec<u8>, JsValue> {
        self.service.validate_activation(&product_id, revision, event_id).await.map(|value| value.encode()).map_err(receiving_error_to_js)
    }

    /// Queue durable activation only after the matching product is ready.
    #[wasm_bindgen(js_name = receivingActivate)]
    pub async fn receiving_activate(&self, product_id: String, revision: u64, event_id: String) -> Result<Vec<u8>, JsValue> {
        self.service.activate(&product_id, revision, event_id).await.map(|value| value.encode()).map_err(receiving_error_to_js)
    }

    /// Revoke locally before logout or destructive identity erasure.
    #[wasm_bindgen(js_name = receivingRevoke)]
    pub async fn receiving_revoke(&self, product_id: String) -> Result<(), JsValue> {
        self.service.revoke(&product_id).await.map_err(receiving_error_to_js)
    }

    /// Queue synchronization after the host durably changes the selected transport.
    #[wasm_bindgen(js_name = receivingMarkTransportChanged)]
    pub async fn receiving_mark_transport_changed(&self, product_id: String) -> Result<(), JsValue> {
        self.service.mark_transport_changed(&product_id).await.map_err(receiving_error_to_js)
    }
}

impl WasmNotificationReceiver {
    async fn command_inner(&self, authority: &[u8], action: u8, payload: &[u8]) -> Result<Vec<u8>, Error> {
        if payload.len() > 2 * 1024 * 1024 || authority.len() > 4096 {
            return Err(Error::Capacity);
        }
        fn request<T: Decode>(payload: &[u8]) -> Result<T, Error> {
            decode_exact(payload).map_err(|reason| Error::InvalidRequest { reason })
        }
        let execution = self.service.for_execution(request::<ReceivingAuthority>(authority)?);
        match action {
            2 => {
                request::<()>(payload)?;
                Ok(execution.status().await.encode())
            }
            3 => {
                let value: latest::HostNotificationReplaceReceiverRequest = request(payload)?;
                Ok(execution.replace(value.expected_revision, value.watches).await.encode())
            }
            4 => {
                let value: latest::HostNotificationDisableReceiverRequest = request(payload)?;
                Ok(execution.disable(value.expected_revision).await.encode())
            }
            5 => {
                let value: latest::HostNotificationRecordReceiptRequest = request(payload)?;
                Ok(execution.receipt(value.revision, value.watch_id, value.event_id, value.kind).await.encode())
            }
            6 => {
                let value: latest::HostNotificationReceiverEventsRequest = request(payload)?;
                Ok(execution.events(value.after_sequence).await.encode())
            }
            7 => {
                let value: latest::HostNotificationAcknowledgeReceiverEventRequest = request(payload)?;
                Ok(execution.acknowledge(value.sequence).await.encode())
            }
            _ => Err(Error::InvalidRequest { reason: "unknown receiving action".into() }),
        }
    }
}
