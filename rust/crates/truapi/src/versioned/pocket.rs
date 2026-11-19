//! Versioned wrappers for [`Pocket`](crate::api::Pocket) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostPocketListSubscribeRequest { V1 }
	pub enum HostPocketListSubscribeItem { V1 => v01::HostPocketListSubscribeItem }
	pub enum HostPocketListSubscribeError { V1 => v01::GenericError }
	pub enum HostPocketRemoveCardRequest { V1 => v01::HostPocketRemoveCardRequest }
	pub enum HostPocketRemoveCardResponse { V1 }
	pub enum HostPocketRemoveCardError { V1 => v01::HostPocketRemoveCardError }
}

#[cfg(test)]
mod tests;
