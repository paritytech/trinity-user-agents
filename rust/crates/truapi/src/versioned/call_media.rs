//! Versioned wrappers for [`CallMedia`](crate::api::CallMedia) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostCallMediaStartRequest { V1 => v01::HostCallMediaStartRequest }
	pub enum HostCallMediaSignalRequest { V1 => v01::HostCallMediaSignalRequest }
	pub enum HostCallMediaStopRequest { V1 => v01::HostCallMediaStopRequest }
	pub enum HostCallMediaResponse { V1 => v01::HostCallMediaResponse }
	pub enum HostCallMediaError { V1 => v01::HostCallMediaError }
}
