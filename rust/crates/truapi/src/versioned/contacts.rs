//! Versioned wrappers for [`Contacts`](crate::api::Contacts) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostContactsPickRequest { V1 => v01::HostContactsPickRequest }
	pub enum HostContactsPickResponse { V1 => v01::HostContactsPickResponse }
	pub enum HostContactsPickError { V1 => v01::HostContactsPickError }
}
