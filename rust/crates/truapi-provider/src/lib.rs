//! Network provider backends for the [`crate::platform::ChainProvider`]
//! capability, shared across every host platform.
//!
//! [`EmbeddedChainProvider`] maps chain genesis hashes to a per-chain
//! [`ChainSource`]. Backends hand the caller the raw JSON-RPC string pipe the
//! trait demands; request correlation and subscription routing stay with the
//! consumer (truapi's `HostRpcClient`).
//!
//! Per-target backend matrix:
//!
//! - `ws` feature — `ChainSource::RpcNode`, a remote JSON-RPC node over
//!   WebSocket. On native targets it runs on a jsonrpsee transport and needs
//!   an ambient tokio runtime; on `wasm32` the same API is served by the
//!   browser's `WebSocket`.
//! - `smoldot` feature — `ChainSource::LightClient`, an embedded
//!   [smoldot](https://github.com/paritytech/smoldot) light client. On native
//!   targets it runs on smoldot's default platform (OS threads, TCP + plain
//!   WebSocket dialing), which declines `wss`, so those bootnodes are reached
//!   through the loopback TLS tunnels in `wss_tunnel`; on `wasm32` it runs on a
//!   vendored browser platform (JS event loop, browser `WebSocket`, which
//!   speaks TLS itself).
//!   `EmbeddedChainProvider::lifecycle` reports its sync progress per chain.
//! - `networks` feature — a bundled catalog so `connect(genesis_hash)`
//!   resolves the whole network (relay wiring + statement placement included)
//!   from the genesis hash alone, with no prior registration.
//! - `js` feature — a JavaScript-facing API (the `js` module) on `wasm32`, so web
//!   hosts can consume the provider directly without a Rust caller.

// The `uniffi` feature pulls in UniFFI's generated scaffolding, which contains
// `unsafe` extern-"C" glue; scope the allowance to that feature so every other
// build keeps the crate's no-unsafe guarantee (see `[lints] unsafe_code`).
#![cfg_attr(
    all(feature = "uniffi", not(target_arch = "wasm32")),
    allow(unsafe_code)
)]

// Without a backend feature the crate carries only the `platform` interfaces,
// which is how truapi depends on it.
#[cfg(any(feature = "ws", feature = "smoldot"))]
mod config;
#[cfg(feature = "smoldot")]
mod connection_types;
mod error;
#[cfg(all(feature = "uniffi", not(target_arch = "wasm32")))]
mod ffi;
#[cfg(all(feature = "js", target_arch = "wasm32"))]
pub mod js;
#[cfg(feature = "smoldot")]
mod lifecycle;
#[cfg(feature = "smoldot")]
mod light;
#[cfg(all(test, feature = "smoldot", not(target_arch = "wasm32")))]
mod light_platform_test;
#[cfg(all(feature = "smoldot", target_arch = "wasm32"))]
mod light_platform_web;
#[cfg(all(feature = "js", target_arch = "wasm32"))]
mod logging;
#[cfg(all(feature = "smoldot", not(target_arch = "wasm32")))]
mod logging_native;
#[cfg(feature = "networks")]
mod networks;
pub mod platform;
#[cfg(any(feature = "ws", feature = "smoldot"))]
mod provider;
#[cfg(feature = "smoldot")]
mod storage;
#[cfg(all(feature = "ws", not(target_arch = "wasm32")))]
mod ws;
#[cfg(all(feature = "ws", target_arch = "wasm32"))]
#[path = "ws_web.rs"]
mod ws;
#[cfg(all(feature = "smoldot", not(target_arch = "wasm32")))]
mod wss_tunnel;

#[cfg(any(feature = "ws", feature = "smoldot"))]
pub use config::ChainSource;
#[cfg(feature = "smoldot")]
pub use config::LightClientBuilder;
#[cfg(all(feature = "smoldot", target_arch = "wasm32"))]
pub use connection_types::ConnectionTypes;
pub use error::ProviderError;
#[cfg(feature = "smoldot")]
pub use lifecycle::{ChainHealth, ChainLifecycle, ChainPhase, StallReason};
#[cfg(feature = "networks")]
pub use networks::{NetworkChains, known_networks};
#[cfg(any(feature = "ws", feature = "smoldot"))]
pub use provider::{EmbeddedChainProvider, EmbeddedChainProviderBuilder};
#[cfg(feature = "smoldot")]
pub use storage::{StorageClient, StorageClientError};

#[cfg(all(feature = "uniffi", not(target_arch = "wasm32")))]
uniffi::setup_scaffolding!();
