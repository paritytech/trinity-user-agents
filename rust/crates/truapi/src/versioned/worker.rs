//! Versioned wrappers for [`Worker`](crate::api::Worker) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostWorkerRetainReferenceRequest { V1 }
	pub enum HostWorkerRetainReferenceItem { V1 }
	pub enum HostWorkerRetainReferenceError { V1 => v01::WorkerReferenceError }
}
