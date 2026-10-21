//! Versioned wrappers for [`Usernames`](crate::api::Usernames) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostUsernamesSearchRequest { V1 => v01::HostUsernamesSearchRequest }
	pub enum HostUsernamesSearchResponse { V1 => v01::HostUsernamesSearchResponse }
	pub enum HostUsernamesSearchError { V1 => v01::HostUsernamesSearchError }
}
