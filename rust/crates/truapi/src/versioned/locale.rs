//! Versioned wrappers for [`Locale`](crate::api::Locale) methods.

use crate::{v01, v02};
use crate::versioned::{FromLatest, IntoLatest};

truapi_macros::versioned_type! {
    pub enum HostLocaleSubscribeRequest { V1, V2 }
    pub enum HostLocaleSubscribeItem {
        V1 => v01::HostLocaleSubscribeItem,
        V2 => v02::HostLocaleSubscribeItem,
    }
    pub enum HostLocaleSubscribeError {
        V1 => v01::GenericError,
        V2 => v01::GenericError,
    }
    pub enum HostLocaleLocalizeTimestampsRequest { V1 => v02::HostLocaleLocalizeTimestampsRequest }
    pub enum HostLocaleLocalizeTimestampsResponse { V1 => v02::HostLocaleLocalizeTimestampsResponse }
    pub enum HostLocaleLocalizeTimestampsError { V1 => v01::GenericError }
}

impl IntoLatest for HostLocaleSubscribeRequest {
    fn into_latest(self) -> Self::Latest {}
}

impl IntoLatest for HostLocaleSubscribeItem {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(item) => v02::HostLocaleSubscribeItem {
                language_tag: item.language_tag,
                time_zone: None,
            },
            Self::V2(item) => item,
        }
    }
}

impl FromLatest for HostLocaleSubscribeItem {
    fn from_latest(item: Self::Latest, target: u8) -> Self {
        if target >= 2 {
            Self::V2(item)
        } else {
            Self::V1(v01::HostLocaleSubscribeItem { language_tag: item.language_tag })
        }
    }
}

impl IntoLatest for HostLocaleSubscribeError {
    fn into_latest(self) -> Self::Latest {
        match self {
            Self::V1(error) | Self::V2(error) => error,
        }
    }
}

impl FromLatest for HostLocaleSubscribeError {
    fn from_latest(error: Self::Latest, target: u8) -> Self {
        if target >= 2 { Self::V2(error) } else { Self::V1(error) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_host_locale_does_not_invent_a_time_zone() {
        let locale = HostLocaleSubscribeItem::V1(v01::HostLocaleSubscribeItem {
            language_tag: "en-US".into(),
        }).into_latest();
        assert_eq!(locale.language_tag, "en-US");
        assert_eq!(locale.time_zone, None);
    }

    #[test]
    fn an_old_subscriber_keeps_its_language_without_new_wire_fields() {
        let locale = v02::HostLocaleSubscribeItem {
            language_tag: "fr-CA".into(),
            time_zone: Some("America/Toronto".into()),
        };
        assert_eq!(
            HostLocaleSubscribeItem::from_latest(locale.clone(), 1),
            HostLocaleSubscribeItem::V1(v01::HostLocaleSubscribeItem { language_tag: "fr-CA".into() }),
        );
        assert_eq!(HostLocaleSubscribeItem::from_latest(locale.clone(), 2).into_latest(), locale);
    }
}
