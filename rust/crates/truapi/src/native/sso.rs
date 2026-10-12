//! Wallet and peer lifetimes for externally owned SSO transports.

use parity_scale_codec::Encode;

use super::HostRejection;
use crate::host_internal::sso_messages::decode_remote_message;
use crate::runtime::SsoAccountHolderService;
use crate::runtime::sso_service::Dispatch;

/// Result for a caller that owns the authenticated SSO transport.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum SsoRequestOutcome {
    /// Response to post through the same peer's transport.
    Response {
        /// SCALE-encoded `RemoteMessage`, forwarded without interpretation.
        message: Vec<u8>,
    },
    /// The peer ended the session; the caller removes its transport and records.
    Disconnected,
    /// No response to post.
    Ignored,
}

/// Incoming requests and cancellation for one authenticated peer.
#[derive(uniffi::Object)]
pub struct NativeSsoAccountHolderService {
    service: SsoAccountHolderService,
}

impl NativeSsoAccountHolderService {
    /// Retain the peer binding verified by the runtime.
    pub fn new(service: SsoAccountHolderService) -> Self {
        Self { service }
    }
}

#[uniffi::export]
impl NativeSsoAccountHolderService {
    /// Apply Cancel immediately; other messages remain in the transport's request queue.
    pub fn handle_sso_control(
        &self,
        message: Vec<u8>,
    ) -> Result<Option<SsoRequestOutcome>, HostRejection> {
        let message = decode_remote_message(&message).map_err(reject)?;
        Ok(self.service.handle_control(&message).map(outcome))
    }

    /// Answer one decrypted SCALE message using this peer's original wallet activation.
    pub async fn handle_sso_request(
        &self,
        message: Vec<u8>,
    ) -> Result<SsoRequestOutcome, HostRejection> {
        let message = decode_remote_message(&message).map_err(reject)?;
        Ok(outcome(self.service.answer(message).await.map_err(reject)?))
    }
}

fn reject(error: impl core::fmt::Display) -> HostRejection {
    HostRejection::Rejected {
        reason: error.to_string(),
    }
}

fn outcome(dispatch: Dispatch) -> SsoRequestOutcome {
    match dispatch {
        Dispatch::Response(answer) => SsoRequestOutcome::Response {
            message: answer.message.encode(),
        },
        Dispatch::Disconnected => SsoRequestOutcome::Disconnected,
        Dispatch::NotARequest(_) | Dispatch::Withdraw(_) | Dispatch::Withdrawn => {
            SsoRequestOutcome::Ignored
        }
    }
}
