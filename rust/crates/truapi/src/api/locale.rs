//! Unified [`Locale`] trait.

use crate::versioned::locale::{
    HostLocaleSubscribeError, HostLocaleSubscribeItem, HostLocaleSubscribeRequest,
};
use crate::{CallContext, CallError, Subscription};
use crate::{wasm_env, wire, wire_trait};

/// Host locale subscription.
#[wasm_env]
#[wire_trait(id = 16)]
#[crate::async_trait]
pub trait Locale: Send + Sync {
    /// Subscribe to the host's selected locale.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const locale = await firstValueFrom(
    ///   from(truapi.locale.subscribe()),
    /// );
    /// console.log("locale received:", locale.languageTag);
    /// ```
    #[wire(id = 0)]
    async fn subscribe(
        &self,
        _cx: &CallContext,
        _request: HostLocaleSubscribeRequest,
    ) -> Subscription<HostLocaleSubscribeItem, CallError<HostLocaleSubscribeError>> {
        Subscription::interrupted(CallError::unavailable())
    }
}
