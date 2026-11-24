//! Versioned wrappers for [`Notifications`](crate::api::Notifications) methods.
//!
//! v0.2 lets a push notification name its title. A v0.1 caller names none, so
//! upgrading its request fills `title` with `None`, which is what v0.1 meant.

use crate::{
	v01, v02,
	versioned::{FromLatest, IntoLatest},
};

truapi_macros::versioned_type! {
	pub enum HostPushNotificationRequest {
		V1 => v01::HostPushNotificationRequest,
		V2 => v02::HostPushNotificationRequest,
	}
	pub enum HostPushNotificationResponse {
		V1 => v01::HostPushNotificationResponse,
		V2 => v01::HostPushNotificationResponse,
	}
	pub enum HostPushNotificationError {
		V1 => v01::HostPushNotificationError,
		V2 => v01::HostPushNotificationError,
	}
	pub enum HostPushNotificationCancelRequest { V1 => v01::HostPushNotificationCancelRequest }
	pub enum HostPushNotificationCancelResponse { V1 }
	pub enum HostPushNotificationCancelError { V1 => v01::GenericError }
}

impl IntoLatest for HostPushNotificationRequest {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(v01::HostPushNotificationRequest { text, deeplink, scheduled_at }) => {
				v02::HostPushNotificationRequest { text, deeplink, scheduled_at, title: None }
			},
			Self::V2(latest) => latest,
		}
	}
}

// The response and error keep their v0.1 shape. They gain a V2 variant to keep
// the version uniform across its request, response and error.

impl IntoLatest for HostPushNotificationResponse {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(payload) | Self::V2(payload) => payload,
		}
	}
}

impl FromLatest for HostPushNotificationResponse {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

impl IntoLatest for HostPushNotificationError {
	fn into_latest(self) -> Self::Latest {
		match self {
			Self::V1(payload) | Self::V2(payload) => payload,
		}
	}
}

impl FromLatest for HostPushNotificationError {
	fn from_latest(latest: Self::Latest, target: u8) -> Self {
		if target >= 2 { Self::V2(latest) } else { Self::V1(latest) }
	}
}

#[cfg(test)]
mod tests;
