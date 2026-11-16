//! Capability implementations for the product runtime.

mod account;
mod chain;
mod expanded_card;
mod game;
mod nft_purse;
#[cfg(not(target_arch = "wasm32"))]
mod coin_payment;
mod payment;
mod platform;
mod preimage;
mod resources;
mod scanner;
mod signing;

/// The browser runs no coinage layer, so every `CoinPayment` call is
/// unavailable there.
#[cfg(target_arch = "wasm32")]
#[async_trait::async_trait]
impl<H: super::AccountHolder> truapi::api::CoinPayment for super::ProductRuntimeHost<H> {}
