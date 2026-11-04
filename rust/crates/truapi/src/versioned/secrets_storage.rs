//! Versioned wrappers for [`SecretsStorage`](crate::api::SecretsStorage) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostSecretsStorageWriteRequest { V1 => v01::HostSecretsStorageWriteRequest }
	pub enum HostSecretsStorageWriteResponse { V1 }
	pub enum HostSecretsStorageWriteError { V1 => v01::HostSecretsStorageWriteError }
	pub enum HostSecretsStorageReadRequest { V1 => v01::HostSecretsStorageReadRequest }
	pub enum HostSecretsStorageReadResponse { V1 => v01::HostSecretsStorageReadResponse }
	pub enum HostSecretsStorageReadError { V1 => v01::HostSecretsStorageLookupError }
	pub enum HostSecretsStorageClearRequest { V1 => v01::HostSecretsStorageClearRequest }
	pub enum HostSecretsStorageClearResponse { V1 }
	pub enum HostSecretsStorageClearError { V1 => v01::HostSecretsStorageLookupError }
}
