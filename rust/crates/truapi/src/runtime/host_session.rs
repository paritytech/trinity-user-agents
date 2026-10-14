/// Host login, connection state and identity presentation.
#[crate::platform::async_trait]
pub trait HostSession: Send + Sync {
	/// Session state shared with this authority.
	fn session_state(&self) -> Arc<SessionState>;

	/// Request account connection for the calling product.
	async fn request_login(
		&self,
		product: &ProductContext,
	) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>>;

	/// Disconnect the current account-authority session.
	async fn disconnect(&self);
}
