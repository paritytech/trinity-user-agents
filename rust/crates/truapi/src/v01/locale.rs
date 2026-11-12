use parity_scale_codec::{Decode, Encode};

/// Locale the host currently presents its interface in, pushed to subscribers.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostLocaleSubscribeItem {
	/// BCP 47 language tag, such as `en`, `pt-BR` or `zh-Hans`. The set is
	/// open: a product that does not ship the tag chooses its own fallback.
	pub language_tag: String,
}
