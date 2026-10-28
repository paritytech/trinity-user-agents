//! Unified [`SecretsStorage`] trait.

use crate::{
	CallContext, CallError,
	versioned::secrets_storage::{
		HostSecretsStorageClearError, HostSecretsStorageClearRequest,
		HostSecretsStorageClearResponse, HostSecretsStorageReadError,
		HostSecretsStorageReadRequest, HostSecretsStorageReadResponse,
		HostSecretsStorageWriteError, HostSecretsStorageWriteRequest,
		HostSecretsStorageWriteResponse,
	},
	wire, wire_trait,
};

/// Small secrets a product keeps with the host, such as the tokens it
/// obtained from its own services.
///
/// The host keeps each value in the platform's encrypted storage, under an id
/// it derives from the signed-in account, the product and the product's key,
/// so any of the product's executions can read it back, including after a
/// restart, and no product or account can read another's. The host does not
/// interpret values, and drops them all when the user signs out or switches
/// account. A product without a session is answered `Denied`, and a host that
/// cannot keep secrets `Unsupported`.
#[wire_trait(id = 27)]
#[crate::async_trait]
pub trait SecretsStorage: Send + Sync {
	/// Read the secret kept under one of the product's keys.
	///
	/// ```ts
	/// const result = await truapi.secretsStorage.read({ key: "onramp" });
	/// assert(result.isOk(), "read failed:", result);
	/// console.log("secret kept:", result.value.value !== undefined);
	/// ```
	#[wire(id = 0)]
	async fn read(
		&self,
		cx: &CallContext,
		request: HostSecretsStorageReadRequest,
	) -> Result<HostSecretsStorageReadResponse, CallError<HostSecretsStorageReadError>>;

	/// Keep a secret under one of the product's keys, replacing any already
	/// kept under it.
	///
	/// ```ts
	/// const result = await truapi.secretsStorage.write({
	///   key: "onramp",
	///   value: new TextEncoder().encode("eyJhbGciOiJIUzI1NiJ9.e30.signature"),
	/// });
	/// assert(result.isOk(), "write failed:", result);
	/// console.log("secret kept");
	/// ```
	#[wire(id = 1)]
	async fn write(
		&self,
		cx: &CallContext,
		request: HostSecretsStorageWriteRequest,
	) -> Result<HostSecretsStorageWriteResponse, CallError<HostSecretsStorageWriteError>>;

	/// Drop the secret kept under one of the product's keys. Dropping one that
	/// is not kept succeeds.
	///
	/// ```ts
	/// const result = await truapi.secretsStorage.clear({ key: "onramp" });
	/// assert(result.isOk(), "clear failed:", result);
	/// console.log("secret dropped");
	/// ```
	#[wire(id = 2)]
	async fn clear(
		&self,
		cx: &CallContext,
		request: HostSecretsStorageClearRequest,
	) -> Result<HostSecretsStorageClearResponse, CallError<HostSecretsStorageClearError>>;
}
