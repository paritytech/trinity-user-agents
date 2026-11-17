//! Versioned wrappers for [`Game`](crate::api::Game) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostRemindNextGameRequest { V1 => v01::HostRemindNextGameRequest }
	pub enum HostRemindNextGameResponse { V1 }
	pub enum HostRemindNextGameError { V1 => v01::HostRemindNextGameError }
	pub enum HostCancelNextGameRequest { V1 => v01::HostCancelNextGameRequest }
	pub enum HostCancelNextGameResponse { V1 }
	pub enum HostCancelNextGameError { V1 => v01::GenericError }
}

#[cfg(test)]
mod tests;
