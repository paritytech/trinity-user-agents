//! Product-facing SecretsStorage capability adapter. Values are core secrets,
//! indexed in the core database and kept in the host's encrypted storage, which
//! the browser core does not have, so there every call answers `Unsupported`,
//! as on a native host that installed neither.

#[cfg(not(target_arch = "wasm32"))]
mod served;

#[cfg(target_arch = "wasm32")]
mod browser {
	use truapi::{
		CallContext, CallError,
		api::SecretsStorage,
		versioned::secrets_storage::{
			HostSecretsStorageClearError, HostSecretsStorageClearRequest,
			HostSecretsStorageClearResponse, HostSecretsStorageReadError,
			HostSecretsStorageReadRequest, HostSecretsStorageReadResponse,
			HostSecretsStorageWriteError, HostSecretsStorageWriteRequest,
			HostSecretsStorageWriteResponse,
		},
	};

	use crate::runtime::ProductRuntimeHost;

	#[truapi::async_trait]
	impl SecretsStorage for ProductRuntimeHost {
		async fn read(
			&self,
			_cx: &CallContext,
			_request: HostSecretsStorageReadRequest,
		) -> Result<HostSecretsStorageReadResponse, CallError<HostSecretsStorageReadError>> {
			Err(CallError::Unsupported)
		}

		async fn write(
			&self,
			_cx: &CallContext,
			_request: HostSecretsStorageWriteRequest,
		) -> Result<HostSecretsStorageWriteResponse, CallError<HostSecretsStorageWriteError>> {
			Err(CallError::Unsupported)
		}

		async fn clear(
			&self,
			_cx: &CallContext,
			_request: HostSecretsStorageClearRequest,
		) -> Result<HostSecretsStorageClearResponse, CallError<HostSecretsStorageClearError>> {
			Err(CallError::Unsupported)
		}
	}
}
