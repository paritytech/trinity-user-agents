use std::sync::Arc;

use crate::host_logic::session::SessionState;
use crate::platform::ProductContext;
use truapi::CallError;
use truapi::latest::{HostRequestLoginError, HostRequestLoginResponse};

/// Host login, connection state and identity presentation.
#[crate::platform::async_trait]
pub trait HostSession: Send + Sync {
    /// Canonical state shared by connection-status consumers.
    fn session_state(&self) -> Arc<SessionState>;

    /// Start or join login for this product without changing wallet ownership.
    async fn request_login(
        &self,
        product: &ProductContext,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>>;

    /// Disconnect the active host session and revoke its local grants.
    async fn disconnect(&self);

    /// Resolve the current session's primary username without prompting.
    async fn primary_username(&self) -> Option<String>;
}
