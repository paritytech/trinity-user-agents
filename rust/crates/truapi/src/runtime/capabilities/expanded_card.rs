//! Product-facing expanded card capability adapter.

use truapi::api::ExpandedCard;

use crate::runtime::ProductRuntimeHost;

#[truapi::async_trait]
impl ExpandedCard for ProductRuntimeHost {}
