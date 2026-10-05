use std::path::PathBuf;

use crate::platform::{
    HostInfo, PlatformInfo, ProductContext, ProductExecutionKind, RuntimeConfigValidationError,
    SigningHostConfig,
};
use truapi::latest::HostPlatform;



/// Process-owned native host configuration shared by every product execution.
#[derive(Debug, Clone, uniffi::Record)]
pub struct HostRuntimeConfig {
    /// Host name shown by the wallet during SSO pairing.
    pub host_name: String,
    /// Optional host icon URL shown by the wallet during SSO pairing.
    #[uniffi(default)]
    pub host_icon: Option<String>,
    /// Optional host version shown by the wallet during SSO pairing.
    #[uniffi(default)]
    pub host_version: Option<String>,
    /// Optional platform/browser name shown by the wallet during SSO pairing.
    #[uniffi(default)]
    pub platform_type: Option<String>,
    /// Optional platform/browser version shown by the wallet during SSO pairing.
    #[uniffi(default)]
    pub platform_version: Option<String>,
    /// People-chain genesis hash. Must be exactly 32 bytes.
    pub people_chain_genesis_hash: Vec<u8>,
    /// Bulletin-chain genesis hash. Must be exactly 32 bytes.
    pub bulletin_chain_genesis_hash: Vec<u8>,
    /// Asset Hub genesis hash, where the dotNS contracts are deployed. Must be
    /// exactly 32 bytes.
    ///
    /// Product manifests are read from dotNS, so this is what makes a
    /// `trustedProducts` grant resolvable. 32 zero bytes says this host has no
    /// Asset Hub; grants already in the manifest cache stay honoured until they
    /// expire.
    pub asset_hub_chain_genesis_hash: Vec<u8>,
    /// The network's dotNS TLD without the leading dot (`dot`, `paseo`,
    /// `testnet`). The wallet's reserved identities are derived under it:
    /// `uid.<suffix>` for the identity account, `peopl.<suffix>` for the person
    /// ring-VRF keys. Read it from the network the host is configured for, the
    /// way the host's own onboarding does; a wrong value derives a different
    /// person from the same seed.
    pub network_suffix: String,
    /// Existing, writable directory for core-owned databases, kept out of
    /// device backups. The runtime opens its database there at startup.
    pub database_directory: String,
    /// Optional local signing-host secret material (raw BIP-39 entropy).
    #[uniffi(default)]
    pub local_session_secret: Option<Vec<u8>>,
    /// Optional lite username attached to the local signing-host session.
    #[uniffi(default)]
    pub local_session_lite_username: Option<String>,
}

/// Trusted identity attached by a native host to one executable connection.
#[derive(Debug, Clone, uniffi::Record)]
pub struct ProductExecutionConfig {
    /// Canonical product identifier used for policy, storage, and derivation.
    pub product_id: String,
    /// Trusted executable kind selected before product code starts.
    pub execution_kind: ProductExecutionKind,
}

/// [`HostRuntimeConfig`] once validated, in the shape the runtime is built from.
#[derive(Debug)]
pub struct NativeResolvedHostRuntimeConfig {
    /// Configuration the signing-host runtime runs with.
    pub signing: SigningHostConfig,
    /// Entropy to activate a local signing session with at construction.
    pub local_session_secret: Option<Vec<u8>>,
    /// Lite username attached to that local session.
    pub local_session_lite_username: Option<String>,
    /// Directory the core database lives in.
    pub database_directory: PathBuf,
}

/// Why a native runtime or product configuration was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum NativeRuntimeConfigError {
    /// A configuration field is invalid; `reason` names the field and value.
    #[error("{reason}")]
    Invalid {
        /// Which field was refused, and why.
        reason: String,
    },
    /// The core runtime's worker threads could not be started.
    #[error("core runtime unavailable: {reason}")]
    RuntimeUnavailable {
        /// Why the runtime failed to start.
        reason: String,
    },
    /// The core database could not be opened in `database_directory`.
    #[error("core database unavailable: {reason}")]
    DatabaseUnavailable {
        /// Which directory, and why opening it failed.
        reason: String,
    },
    /// Local signing-host session activation failed.
    #[error("failed to activate local signing session: {reason}")]
    LocalSessionActivation {
        /// Activation failure reason.
        reason: String,
    },
}

impl From<RuntimeConfigValidationError> for NativeRuntimeConfigError {
    fn from(err: RuntimeConfigValidationError) -> Self {
        Self::Invalid {
            reason: err.to_string(),
        }
    }
}

/// A 32-byte genesis hash handed over as bytes, or the error naming `field`.
pub fn genesis_hash(field: &str, bytes: &[u8]) -> Result<[u8; 32], NativeRuntimeConfigError> {
    bytes
        .try_into()
        .map_err(|_| NativeRuntimeConfigError::Invalid {
            reason: format!("{field} must be exactly 32 bytes, got {}", bytes.len()),
        })
}

/// The platform products see in `System::host_info`. The native library ships
/// only inside iOS and Android apps, so the target decides it.
pub fn native_host_platform() -> HostPlatform {
    if cfg!(target_os = "ios") {
        HostPlatform::Ios
    } else if cfg!(target_os = "android") {
        HostPlatform::Android
    } else {
        HostPlatform::Unknown
    }
}

impl TryFrom<HostRuntimeConfig> for NativeResolvedHostRuntimeConfig {
    type Error = NativeRuntimeConfigError;

    fn try_from(config: HostRuntimeConfig) -> Result<Self, Self::Error> {
        let people_chain_genesis_hash =
            genesis_hash("people_chain_genesis_hash", &config.people_chain_genesis_hash)?;
        let bulletin_chain_genesis_hash =
            genesis_hash("bulletin_chain_genesis_hash", &config.bulletin_chain_genesis_hash)?;
        let asset_hub_chain_genesis_hash = genesis_hash(
            "asset_hub_chain_genesis_hash",
            &config.asset_hub_chain_genesis_hash,
        )?;
        let signing = SigningHostConfig::new(
            HostInfo {
                name: config.host_name,
                icon: config.host_icon,
                version: config.host_version,
                platform: native_host_platform(),
            },
            PlatformInfo {
                kind: config.platform_type,
                version: config.platform_version,
            },
            people_chain_genesis_hash,
            bulletin_chain_genesis_hash,
            asset_hub_chain_genesis_hash,
            config.network_suffix,
        )?;
        Ok(Self {
            signing,
            local_session_secret: config.local_session_secret,
            local_session_lite_username: config.local_session_lite_username,
            database_directory: PathBuf::from(config.database_directory),
        })
    }
}

impl From<&ProductContext> for ProductExecutionConfig {
    fn from(product: &ProductContext) -> Self {
        Self {
            product_id: product.product_id.clone(),
            execution_kind: product.execution_kind,
        }
    }
}

impl TryFrom<ProductExecutionConfig> for ProductContext {
    type Error = NativeRuntimeConfigError;

    fn try_from(config: ProductExecutionConfig) -> Result<Self, Self::Error> {
        ProductContext::new_with_execution(config.product_id, config.execution_kind)
            .map_err(NativeRuntimeConfigError::from)
    }
}
