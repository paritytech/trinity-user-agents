//! Versioned wrappers for [`Worker`](crate::api::Worker) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostWorkerBeginOperationRequest { V1 => v01::HostWorkerBeginOperationRequest }
	pub enum HostWorkerBeginOperationResponse { V1 => v01::HostWorkerBeginOperationResponse }
	pub enum HostWorkerBeginOperationError { V1 => v01::HostWorkerOperationError }
	pub enum HostWorkerEndOperationRequest { V1 => v01::HostWorkerEndOperationRequest }
	pub enum HostWorkerEndOperationResponse { V1 }
	pub enum HostWorkerEndOperationError { V1 => v01::HostWorkerOperationError }
}
