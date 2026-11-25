use std::path::PathBuf;

use crate::platform::{
	ExecutableKind, HostIdentity, HostInfo, PlatformInfo, ProductContext, ProductId,
	ReservedValuesPolicy, RuntimeConfigValidationError, StackError,
};
use truapi::{Bytes32, latest::HostPlatform};

/// Process-owned native host configuration shared by every product execution.
#[derive(Clone, uniffi::Record)]
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
	/// The stack the host build runs. Every chain the core uses, the network
	/// `get_chain_info` reports and the TLD the wallet's identities derive
	/// under come from it.
	pub stack: BundledStack,
	/// Existing, writable directory for core-owned databases, kept out of
	/// device backups. The runtime opens its database there at startup.
	pub database_directory: String,
	/// Whether the host draws materials. Off by default, which draws every
	/// `Materials` node plain and answers no to products that ask.
	#[uniffi(default)]
	pub materials_enabled: bool,
	/// Optional local signing-host secret material (raw BIP-39 entropy).
	#[uniffi(default)]
	pub local_session_secret: Option<Vec<u8>>,
	/// Optional lite username attached to the local signing-host session.
	#[uniffi(default)]
	pub local_session_lite_username: Option<String>,
	/// X25519 secret this host announced as its device key before the core
	/// kept one. The core adopts it before activating the local session, so
	/// contacts already addressing it stay reachable; `None` keeps the key the
	/// core holds, or generates one.
	#[uniffi(default)]
	pub device_encryption_key: Option<Bytes32>,
}

/// The stack a host build ships: its pointer, its signed descriptor, and the
/// build's policy on reserved values.
#[derive(Clone, uniffi::Record)]
pub struct BundledStack {
	/// The stack's link, `polkadotapp://stack?key=0x…&url=…`. It must carry a
	/// key.
	pub pointer: String,
	/// The bundled descriptor's bytes, as served: a signed envelope that
	/// verifies under the pointer's key.
	pub descriptor: Vec<u8>,
	/// The build's policy on reserved values.
	pub reserved_values: ReservedValuesPolicy,
}

/// Trusted identity attached by a native host to one executable connection.
#[derive(Clone, uniffi::Record)]
pub struct ProductExecutionConfig {
	/// Canonical product identifier used for policy, storage, and derivation.
	pub product_id: ProductId,
	/// Trusted executable kind selected before product code starts.
	pub execution_kind: ExecutableKind,
}

/// [`HostRuntimeConfig`] once validated, in the shape the runtime is built from.
pub struct NativeResolvedHostRuntimeConfig {
	/// The host the wallet and products see.
	pub host: HostIdentity,
	/// The stack the host build ships. The runtime resolves it when it is
	/// built, and refreshes it while it runs.
	pub stack: BundledStack,
	/// Whether the host draws materials.
	pub materials_enabled: bool,
	/// Entropy to activate a local signing session with at construction.
	pub local_session_secret: Option<Vec<u8>>,
	/// Lite username attached to that local session.
	pub local_session_lite_username: Option<String>,
	/// Device encryption secret to adopt before that session activates.
	pub device_encryption_key: Option<Bytes32>,
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
	/// The device encryption key the host handed over could not be stored.
	#[error("failed to adopt the device encryption key: {reason}")]
	DeviceEncryptionKeyAdoption {
		/// Why storing the key failed.
		reason: String,
	},
}

impl From<RuntimeConfigValidationError> for NativeRuntimeConfigError {
	fn from(err: RuntimeConfigValidationError) -> Self {
		Self::Invalid { reason: err.to_string() }
	}
}

impl From<StackError> for NativeRuntimeConfigError {
	fn from(err: StackError) -> Self {
		Self::Invalid { reason: err.to_string() }
	}
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
		let host = HostIdentity::new(
			HostInfo {
				name: config.host_name,
				icon: config.host_icon,
				version: config.host_version,
				platform: native_host_platform(),
			},
			PlatformInfo { kind: config.platform_type, version: config.platform_version },
		)?;
		Ok(Self {
			host,
			stack: config.stack,
			materials_enabled: config.materials_enabled,
			local_session_secret: config.local_session_secret,
			local_session_lite_username: config.local_session_lite_username,
			device_encryption_key: config.device_encryption_key,
			database_directory: PathBuf::from(config.database_directory),
		})
	}
}

impl From<&ProductContext> for ProductExecutionConfig {
	fn from(product: &ProductContext) -> Self {
		Self { product_id: product.product_id.clone(), execution_kind: product.execution_kind }
	}
}

impl From<ProductExecutionConfig> for ProductContext {
	fn from(config: ProductExecutionConfig) -> Self {
		ProductContext::new_with_execution(config.product_id, config.execution_kind)
	}
}
