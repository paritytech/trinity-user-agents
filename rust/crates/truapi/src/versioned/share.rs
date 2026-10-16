//! Versioned wrappers for [`Share`](crate::api::Share) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostShareRequest { V1 => v01::HostShareRequest }
	pub enum HostShareResponse { V1 => v01::HostShareResponse }
	pub enum HostShareError { V1 => v01::HostShareError }
}
