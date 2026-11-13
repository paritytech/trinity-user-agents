//! Versioned wrappers for [`Contacts`](crate::api::Contacts) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostContactsPickRequest { V1 => v01::HostContactsPickRequest }
	pub enum HostContactsPickResponse { V1 => v01::HostContactsPickResponse }
	pub enum HostContactsPickError { V1 => v01::HostContactsPickError }
	pub enum HostContactsUsernamesRequest { V1 => v01::HostContactsUsernamesRequest }
	pub enum HostContactsUsernamesResponse { V1 => v01::HostContactsUsernamesResponse }
	pub enum HostContactsUsernamesError { V1 => v01::HostContactsUsernamesError }
}
