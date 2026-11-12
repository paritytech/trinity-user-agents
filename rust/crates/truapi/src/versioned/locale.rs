//! Versioned wrappers for [`Locale`](crate::api::Locale) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostLocaleSubscribeRequest { V1 }
	pub enum HostLocaleSubscribeItem { V1 => v01::HostLocaleSubscribeItem }
	pub enum HostLocaleSubscribeError { V1 => v01::GenericError }
}
