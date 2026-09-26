//! Profile presenter for the CLI.
//!
//! The CLI has no UI to draw a profile in, so a presentation is accepted and
//! recorded: every `present` (and `present_contact`, which reaches the host as
//! a `present` of the reference the core substituted) is appended to the
//! transcript named by `TRUAPI_PROFILE_LOG`, one JSON object per line, so a
//! battery can assert what the host was handed.
//!
//! The reference is a bearer capability, so the transcript carries its
//! SHA-256 and format prefix, never the reference itself.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use truapi::latest::{HostProfilePresentError, HostProfilePresentRequest};
use truapi_platform::{ProductContext, ProfilePlatform, async_trait};

/// A presenter that shows nothing and remembers everything it was asked.
pub struct CliProfileHost {
    transcript: Option<PathBuf>,
}

impl CliProfileHost {
    /// Build a presenter recording to `TRUAPI_PROFILE_LOG` when that names a
    /// path. The transcript is truncated at startup so a run never reads an
    /// earlier run's presentations as its own.
    pub fn from_env() -> Arc<Self> {
        let transcript = std::env::var_os("TRUAPI_PROFILE_LOG").map(PathBuf::from);
        if let Some(path) = transcript.as_ref()
            && let Err(error) = std::fs::write(path, b"")
        {
            tracing::warn!(?path, %error, "profile transcript could not be truncated");
        }
        Arc::new(Self { transcript })
    }

    fn record(&self, line: serde_json::Value) {
        let Some(path) = self.transcript.as_ref() else {
            return;
        };
        let appended = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| file.write_all(format!("{line}\n").as_bytes()));
        if let Err(error) = appended {
            tracing::warn!(?path, %error, "profile transcript could not be appended to");
        }
    }
}

/// The part of a reference before its first `:` or `#`, which names its format
/// without revealing its secret.
fn reference_kind(reference: &str) -> &str {
    let end = reference
        .find(['#', ':'])
        .unwrap_or(reference.len())
        .min(32);
    &reference[..end]
}

#[async_trait]
impl ProfilePlatform for CliProfileHost {
    async fn present_profile(
        &self,
        product: &ProductContext,
        request: HostProfilePresentRequest,
    ) -> Result<(), HostProfilePresentError> {
        let digest = hex::encode(Sha256::digest(request.reference.as_bytes()));
        tracing::info!(product = %product.product_id, %digest, "profile presented");
        self.record(serde_json::json!({
            "event": "present",
            "product": product.product_id,
            "kind": reference_kind(&request.reference),
            "sha256": digest,
        }));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_kind_never_includes_its_secret() {
        assert_eq!(reference_kind("seity-contacts:v1:abcd"), "seity-contacts");
        assert_eq!(reference_kind("bafk2bz#00ff"), "bafk2bz");
        assert_eq!(reference_kind(&"a".repeat(80)).len(), 32);
    }
}
