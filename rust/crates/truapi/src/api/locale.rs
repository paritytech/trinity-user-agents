//! Unified [`Locale`] trait.

use crate::versioned::locale::{
    HostLocaleLocalizeTimestampsError, HostLocaleLocalizeTimestampsRequest,
    HostLocaleLocalizeTimestampsResponse, HostLocaleSubscribeError, HostLocaleSubscribeItem,
    HostLocaleSubscribeRequest,
};
use crate::{CallContext, CallError, Subscription};
use crate::{wire, wire_trait};

/// Host locale subscription.
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

    /// Localize a bounded batch of UTC instants in a host locale snapshot.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const locale = await firstValueFrom(from(truapi.locale.subscribe()));
    /// if (locale.timeZone !== undefined) {
    ///   const localized = await truapi.locale.localizeTimestamps({
    ///     timestampsMs: [BigInt(Date.now())],
    ///     languageTag: locale.languageTag,
    ///     timeZone: locale.timeZone,
    ///   });
    ///   if (localized.isOk()) console.log(localized.value.timestamps[0]?.dateTime);
    /// }
    /// ```
    #[wire(id = 1)]
    async fn localize_timestamps(
        &self,
        _cx: &CallContext,
        _request: HostLocaleLocalizeTimestampsRequest,
    ) -> Result<HostLocaleLocalizeTimestampsResponse, CallError<HostLocaleLocalizeTimestampsError>>
    {
        Err(CallError::unavailable())
    }
}
