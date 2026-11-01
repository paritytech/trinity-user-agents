use std::sync::Arc;

use crate::{host_logic::session::SessionState, platform::ProductContext};
use truapi::{
	CallError,
	latest::{HostRequestLoginError, HostRequestLoginResponse},
};

/// Host login and connection state.
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
}
