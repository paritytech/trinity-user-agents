//! Bandersnatch ring-VRF operations, from `truapi-verifiable`.
//!
//! Native builds link it. The browser core loads it as a WASM module of its
//! own, published in the same directory as the core's, once a pairing session connects or when
//! a call first needs a ring-VRF key, whichever comes first: it carries
//! `verifiable`, whose ring prover compiles in 4.5 MiB of powers of tau.

use parity_scale_codec::{Decode, DecodeAll};

use crate::host_internal::sso_messages::RingVrfError;

#[cfg(not(target_arch = "wasm32"))]
use truapi_verifiable as module;

/// Ring domain size 2^11, for on-chain ring exponent 9. `prove` takes the
/// size, as `RingDomainSize::value` gives it, not the power.
pub const DOMAIN_2E11: u32 = 1 << 11;
/// Ring domain size 2^12, for on-chain ring exponent 10.
pub const DOMAIN_2E12: u32 = 1 << 12;
/// Ring domain size 2^16, for on-chain ring exponent 14.
pub const DOMAIN_2E16: u32 = 1 << 16;

/// Ring-VRF operations, from [`load`].
pub struct Vrf(());

/// Ring-VRF operations, which native builds link in.
#[cfg(not(target_arch = "wasm32"))]
pub async fn load() -> Result<Vrf, RingVrfError> {
    Ok(Vrf(()))
}

/// Native builds link the operations in, so there is nothing to prefetch.
#[cfg(not(target_arch = "wasm32"))]
pub fn prefetch(_spawner: &crate::subscription::Spawner) {}

#[cfg(target_arch = "wasm32")]
pub use module::{load, prefetch};

/// The ring member for `entropy`, loading the operations first.
#[cfg(all(target_arch = "wasm32", feature = "test-host"))]
pub async fn ring_vrf_member(entropy: &[u8; 32]) -> Result<[u8; 32], RingVrfError> {
    load().await?.member(entropy)
}

impl Vrf {
    /// The ring member, a 32-byte public key, for `entropy`.
    pub fn member(&self, entropy: &[u8; 32]) -> Result<[u8; 32], RingVrfError> {
        answer(&module::member(entropy))
    }

    /// A signature over `message` with the key for `entropy`.
    pub fn sign(&self, entropy: &[u8; 32], message: &[u8]) -> Result<Vec<u8>, RingVrfError> {
        answer(&module::sign(entropy, message))
    }

    /// The alias of the key for `entropy` in `context`.
    pub fn alias(&self, entropy: &[u8; 32], context: &[u8]) -> Result<[u8; 32], RingVrfError> {
        answer(&module::alias(entropy, context))
    }

    /// Prove, with the key for `entropy`, that `member` belongs to the ring
    /// `members` of the ring domain of size `domain`, in `context`. Returns the
    /// encoded proof and the member's alias in that context.
    pub fn prove(
        &self,
        entropy: &[u8; 32],
        domain: u32,
        member: &[u8; 32],
        members: &[[u8; 32]],
        context: &[u8],
        message: &[u8],
    ) -> Result<(Vec<u8>, [u8; 32]), RingVrfError> {
        answer(&module::prove(
            entropy,
            domain,
            member,
            &members.concat(),
            context,
            message,
        ))
    }

    /// Prove membership once for several 32-byte contexts, returning one alias per context.
    pub fn prove_multi_context(
        &self,
        entropy: &[u8; 32],
        domain: u32,
        member: &[u8; 32],
        members: &[[u8; 32]],
        contexts: &[[u8; 32]],
        message: &[u8],
    ) -> Result<(Vec<u8>, Vec<[u8; 32]>), RingVrfError> {
        answer(&module::prove_multi_context(
            entropy,
            domain,
            member,
            &members.concat(),
            &contexts.concat(),
            message,
        ))
    }
}

/// A `truapi-verifiable` answer: `Result<T, String>`, SCALE-encoded.
fn answer<T: Decode>(encoded: &[u8]) -> Result<T, RingVrfError> {
    Result::<T, String>::decode_all(&mut &encoded[..])
        .map_err(|error| RingVrfError::Unknown {
            reason: format!("undecodable ring-VRF answer: {error}"),
        })?
        .map_err(|reason| RingVrfError::Unknown { reason })
}

#[cfg(target_arch = "wasm32")]
mod module {
    use super::Vrf;

    use futures::lock::Mutex;
    use js_sys::Uint8Array;
    use send_wrapper::SendWrapper;
    use sha2::{Digest, Sha256};
    use std::sync::OnceLock;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;

    use crate::host_internal::sso_messages::RingVrfError;

    // wasm-bindgen writes this snippet to `snippets/<crate>-<hash>/`, two levels
    // below the core's glue, beside which `make wasm` publishes
    // `truapi-verifiable`. The URLs are literals relative to this file so
    // bundlers that follow `new URL(…, import.meta.url)` emit the module.
    #[wasm_bindgen(inline_js = r#"
let module;
export async function read() {
  const url = new URL("../../truapi_verifiable_bg.wasm", import.meta.url);
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: ${response.status} ${response.statusText}`);
  return new Uint8Array(await response.arrayBuffer());
}
export async function start(wasm) {
  const glue = await import(/* @vite-ignore */ new URL("../../truapi_verifiable.js", import.meta.url).href);
  await glue.default({ module_or_path: wasm });
  module = glue;
}
export const member = (...args) => module.member(...args);
export const sign = (...args) => module.sign(...args);
export const alias = (...args) => module.alias(...args);
export const prove = (...args) => module.prove(...args);
export const prove_multi_context = (...args) => module.prove_multi_context(...args);
"#)]
    extern "C" {
        fn read() -> js_sys::Promise;
        fn start(wasm: &Uint8Array) -> js_sys::Promise;
        pub fn member(entropy: &[u8]) -> Vec<u8>;
        pub fn sign(entropy: &[u8], message: &[u8]) -> Vec<u8>;
        pub fn alias(entropy: &[u8], context: &[u8]) -> Vec<u8>;
        pub fn prove_multi_context(
            entropy: &[u8],
            domain: u32,
            member: &[u8],
            members: &[u8],
            contexts: &[u8],
            message: &[u8],
        ) -> Vec<u8>;
        pub fn prove(
            entropy: &[u8],
            domain: u32,
            member: &[u8],
            members: &[u8],
            context: &[u8],
            message: &[u8],
        ) -> Vec<u8>;
    }

    /// SHA-256 of the module `make wasm` built beside this core, the only one
    /// it loads.
    const PINNED: Option<&str> = option_env!("TRUAPI_VERIFIABLE_SHA256");

    /// Whether the module has started, behind an async lock so concurrent
    /// calls wait on one load.
    static STARTED: OnceLock<Mutex<bool>> = OnceLock::new();

    /// Ring-VRF operations, loading the module on first use. A failed load is
    /// not remembered, so a later call tries again.
    pub async fn load() -> Result<Vrf, RingVrfError> {
        let mut started = STARTED.get_or_init(|| Mutex::new(false)).lock().await;
        if !*started {
            SendWrapper::new(start_module())
                .await
                .map_err(|reason| RingVrfError::Unknown { reason })?;
            *started = true;
        }
        Ok(Vrf(()))
    }

    /// Load the module in the background, so a later call does not wait on
    /// it. A call made while it loads waits on this load instead of starting
    /// another.
    pub fn prefetch(spawner: &crate::subscription::Spawner) {
        spawner(Box::pin(async {
            if let Err(error) = load().await {
                tracing::warn!(%error, "truapi-verifiable prefetch failed");
            }
        }));
    }

    async fn start_module() -> Result<(), String> {
        let pinned = PINNED.ok_or("this core was built without truapi-verifiable")?;
        let wasm: Uint8Array = JsFuture::from(read())
            .await
            .map_err(|error| format!("{error:?}"))?
            .unchecked_into();
        if hex::encode(Sha256::digest(wasm.to_vec())) != pinned {
            return Err("truapi-verifiable is not the build this core pins".to_owned());
        }
        JsFuture::from(start(&wasm))
            .await
            .map_err(|error| format!("{error:?}"))?;
        Ok(())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::{DOMAIN_2E11, DOMAIN_2E12, DOMAIN_2E16};
    use verifiable::ring::RingDomainSize;

    #[test]
    fn domain_sizes_are_the_ones_verifiable_accepts() {
        assert_eq!(
            RingDomainSize::try_from(DOMAIN_2E11),
            Ok(RingDomainSize::Domain11)
        );
        assert_eq!(
            RingDomainSize::try_from(DOMAIN_2E12),
            Ok(RingDomainSize::Domain12)
        );
        assert_eq!(
            RingDomainSize::try_from(DOMAIN_2E16),
            Ok(RingDomainSize::Domain16)
        );
    }
}
