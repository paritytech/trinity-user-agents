//! Unified [`Theme`] trait.

use crate::{
	CallContext, CallError, Subscription,
	versioned::theme::{
		HostThemeSubscribeError, HostThemeSubscribeItem, HostThemeSubscribeRequest,
	},
	wire, wire_trait,
};

/// Host theme subscription.
#[wire_trait(id = 15)]
#[crate::async_trait]
pub trait Theme: Send + Sync {
	/// Subscribe to host theme changes.
	///
	/// ```ts
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// const theme = await firstValueFrom(
	///   from(truapi.theme.subscribe()),
	/// );
	/// console.log("theme received:", theme);
	/// ```
	#[wire(id = 0)]
	async fn subscribe(
		&self,
		_cx: &CallContext,
		_request: HostThemeSubscribeRequest,
	) -> Subscription<HostThemeSubscribeItem, CallError<HostThemeSubscribeError>> {
		Subscription::interrupted(CallError::unavailable())
	}
}
