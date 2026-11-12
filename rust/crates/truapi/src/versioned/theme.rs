//! Versioned wrappers for [`Theme`](crate::api::Theme) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostThemeSubscribeRequest { V1 }
	pub enum HostThemeSubscribeItem { V1 => v01::HostThemeSubscribeItem }
	pub enum HostThemeSubscribeError { V1 => v01::GenericError }
}
