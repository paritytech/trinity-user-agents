//! Product-facing NFT purse capability adapter. Purses live in the core
//! database, which the browser core does not have, so there every call
//! answers unavailable.

#[cfg(not(target_arch = "wasm32"))]
mod served;

#[cfg(target_arch = "wasm32")]
impl truapi::api::NftPurse for crate::runtime::ProductRuntimeHost {}
