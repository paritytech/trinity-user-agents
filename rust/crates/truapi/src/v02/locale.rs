use alloc::{string::String, vec::Vec};
use parity_scale_codec::{Decode, Encode};

/// Host language and local time zone, replaced together when either changes.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostLocaleSubscribeItem {
    /// BCP 47 language tag selected by the host.
    pub language_tag: String,
    /// IANA time zone, or absent when the host cannot supply local time.
    pub time_zone: Option<String>,
}

/// Convert UTC instants using a snapshot of the host's locale subscription.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostLocaleLocalizeTimestampsRequest {
    /// At most 128 Unix millisecond instants, no later than year 9999.
    pub timestamps_ms: Vec<u64>,
    /// Language tag from the locale subscription, not a guessed language.
    pub language_tag: String,
    /// Time zone from the locale subscription; evaluated separately at each instant.
    pub time_zone: String,
}

/// One timestamp's calendar identity and presentation in the requested context.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostLocaleLocalizedTimestamp {
    /// Gregorian YYYY-MM-DD local date, independent of display language/calendar.
    pub local_date: String,
    /// Localized short time, including the host language's hour-cycle convention.
    pub time: String,
    /// Localized date label.
    pub date: String,
    /// Localized date and time with a time-zone indication for detail views.
    pub date_time: String,
}

/// Local timestamps in exactly the request's order.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostLocaleLocalizeTimestampsResponse {
    /// One result per requested timestamp; partial success is not returned.
    pub timestamps: Vec<HostLocaleLocalizedTimestamp>,
}
