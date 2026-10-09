//! Versioned wrappers for [`Contacts`](crate::api::Contacts) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostContactsPickRequest { V1 => v01::HostContactsPickRequest }
    pub enum HostContactsPickResponse { V1 => v01::HostContactsPickResponse }
    pub enum HostContactsPickError { V1 => v01::HostContactsPickError }
    pub enum HostContactsPickManyRequest { V1 => v01::HostContactsPickManyRequest }
    pub enum HostContactsPickManyResponse { V1 => v01::HostContactsPickManyResponse }
    pub enum HostContactsPickManyError { V1 => v01::HostContactsPickManyError }
    pub enum HostContactsPlaceLabelsRequest { V1 => v01::HostContactsPlaceLabelsRequest }
    pub enum HostContactsPlaceLabelsResponse { V1 => v01::HostContactsPlaceLabelsResponse }
    pub enum HostContactsPlaceLabelsError { V1 => v01::HostContactsPlaceLabelsError }
}
