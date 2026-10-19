//! Versioned wrappers for [`Push`](crate::api::Push) methods.

use crate::v01;

truapi_macros::versioned_type! {
	pub enum HostPushAddressRequest { V1 => v01::HostPushAddressRequest }
	pub enum HostPushAddressResponse { V1 => v01::HostPushAddressResponse }
	pub enum HostPushAddressError { V1 => v01::HostPushAddressError }
	pub enum HostPushNotifyRequest { V1 => v01::HostPushNotifyRequest }
	pub enum HostPushNotifyResponse { V1 => v01::HostPushNotifyResponse }
	pub enum HostPushNotifyError { V1 => v01::HostPushNotifyError }
}
