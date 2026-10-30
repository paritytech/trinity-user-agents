//! SecretsStorage over products' secrets.

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::SecretsStorage,
	latest,
	versioned::secrets_storage::{
		HostSecretsStorageClearError, HostSecretsStorageClearRequest,
		HostSecretsStorageClearResponse, HostSecretsStorageReadError,
		HostSecretsStorageReadRequest, HostSecretsStorageReadResponse,
		HostSecretsStorageWriteError, HostSecretsStorageWriteRequest,
		HostSecretsStorageWriteResponse,
	},
};

use crate::{
	host_logic::secrets_storage::{MAX_KEYS_PER_PRODUCT, check_write, is_valid_key, secret_id},
	product_secrets::{ProductSecrets, SecretOwner},
	runtime::{ProductRuntimeHost, versioning::version},
};

impl ProductRuntimeHost {
	/// Products' secrets and the signed-in account the secret under `key` is
	/// kept for. Secrets belong to the signed-in account, so without a session
	/// there is nobody to keep them for.
	fn secret_slot<E>(&self, key: &str) -> Result<SecretSlot, CallError<E>> {
		let secrets = self.connection.services.product_secrets().ok_or(CallError::Unsupported)?;
		let session = self.accounts.current_session().ok_or(CallError::Denied)?;
		let id = secret_id(&session.public_key, self.connection.product.product_id.as_str(), key);
		Ok(SecretSlot { secrets, owner: session.public_key, id })
	}

	/// Whether `owner` is still the signed-in account.
	fn is_signed_in(&self, owner: &[u8; 32]) -> bool {
		self.accounts
			.current_session()
			.is_some_and(|session| &session.public_key == owner)
	}
}

/// Products' secrets, the signed-in account, and the id one key's secret is
/// kept under.
struct SecretSlot {
	secrets: ProductSecrets,
	owner: [u8; 32],
	id: [u8; 32],
}

/// The reason for a failure, as the product's catch-all for a read or clear.
fn lookup_failure(error: latest::GenericError) -> CallError<latest::HostSecretsStorageLookupError> {
	CallError::Domain(latest::HostSecretsStorageLookupError::Unknown { reason: error.reason })
}

fn write_failure(error: latest::GenericError) -> CallError<latest::HostSecretsStorageWriteError> {
	CallError::Domain(latest::HostSecretsStorageWriteError::Unknown { reason: error.reason })
}

#[truapi::async_trait]
impl SecretsStorage for ProductRuntimeHost {
	/// Storage the host cannot read is an error, not an absent secret, so a
	/// locked Keychain does not look like a sign-out to the product.
	#[instrument(skip_all, fields(runtime.method = "secrets_storage.read"))]
	async fn read(
		&self,
		_cx: &CallContext,
		request: HostSecretsStorageReadRequest,
	) -> Result<HostSecretsStorageReadResponse, CallError<HostSecretsStorageReadError>> {
		version(request)
			.exec(|latest::HostSecretsStorageReadRequest { key }| async move {
				let SecretSlot { secrets, id, .. } = self.secret_slot(&key)?;
				if !is_valid_key(&key) {
					return Err(CallError::Domain(
						latest::HostSecretsStorageLookupError::InvalidKey,
					));
				}
				let value =
					secrets.get(&id).await.map_err(lookup_failure)?.map(|kept| kept.to_vec());
				Ok(latest::HostSecretsStorageReadResponse { value })
			})
			.await
	}

	/// A key the product has no secret under yet only gets one while the
	/// product keeps fewer than [`MAX_KEYS_PER_PRODUCT`] for the signed-in
	/// account. The count check and the write hold one lock, so concurrent
	/// writes cannot pass the cap together. Dropping secrets at sign-out takes
	/// the same lock, and the session is checked again under it, so a write
	/// racing a sign-out or an account switch cannot outlive it.
	#[instrument(skip_all, fields(runtime.method = "secrets_storage.write"))]
	async fn write(
		&self,
		_cx: &CallContext,
		request: HostSecretsStorageWriteRequest,
	) -> Result<HostSecretsStorageWriteResponse, CallError<HostSecretsStorageWriteError>> {
		version(request)
			.exec(|latest::HostSecretsStorageWriteRequest { key, value }| async move {
				let SecretSlot { secrets, owner, id } = self.secret_slot(&key)?;
				check_write(&key, &value).map_err(CallError::Domain)?;
				let owned_by = SecretOwner {
					product_id: self.connection.product.product_id.as_str(),
					account: &owner,
				};
				let _writes = self.connection.services.secrets_storage_writes().await;
				if !self.is_signed_in(&owner) {
					return Err(CallError::Denied);
				}
				let replacing = secrets.get(&id).await.map_err(write_failure)?.is_some();
				if !replacing &&
					secrets.count(owned_by).await.map_err(write_failure)? >= MAX_KEYS_PER_PRODUCT
				{
					return Err(CallError::Domain(
						latest::HostSecretsStorageWriteError::TooManyKeys {
							max: MAX_KEYS_PER_PRODUCT,
						},
					));
				}
				secrets.put(&id, owned_by, &value).await.map_err(write_failure)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "secrets_storage.clear"))]
	async fn clear(
		&self,
		_cx: &CallContext,
		request: HostSecretsStorageClearRequest,
	) -> Result<HostSecretsStorageClearResponse, CallError<HostSecretsStorageClearError>> {
		version(request)
			.exec(|latest::HostSecretsStorageClearRequest { key }| async move {
				let SecretSlot { secrets, id, .. } = self.secret_slot(&key)?;
				if !is_valid_key(&key) {
					return Err(CallError::Domain(
						latest::HostSecretsStorageLookupError::InvalidKey,
					));
				}
				secrets.delete(&id).await.map_err(lookup_failure)
			})
			.await
	}
}
