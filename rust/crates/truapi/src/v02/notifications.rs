//! Notification messages v0.2 redefines: a push notification may name its
//! own title.

use parity_scale_codec::{Decode, Encode};

/// Push notification payload.
///
/// When `scheduled_at` is `Some`, the notification is deferred to the given
/// wall-clock instant (Unix milliseconds UTC). `None` fires immediately.
///
/// `title` replaces the product id the host shows as the notification's
/// title. Only a trusted product may set one: the core clears it for every
/// other caller before the host sees the request, so no product can present
/// a notification as coming from someone else. The core trims whitespace and
/// zero width characters from the title it keeps, and clears one that nothing
/// is left of, so a host never shows a blank one.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostPushNotificationRequest {
	/// Notification text.
	pub text: String,
	/// Optional URL to open on tap.
	pub deeplink: Option<String>,
	/// Optional Unix timestamp in milliseconds (UTC) at which the notification
	/// should fire. `None` fires immediately.
	pub scheduled_at: Option<u64>,
	/// Notification title. `None` shows the product id.
	#[cfg_attr(not(target_arch = "wasm32"), uniffi(default))]
	pub title: Option<String>,
}
