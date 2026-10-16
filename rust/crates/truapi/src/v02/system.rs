use parity_scale_codec::{Decode, Encode};

use crate::v01::HostPlatform;

/// Identity and version of the host currently running the product.
///
/// Reported by [`crate::api::System::host_info`] so a product knows which host
/// (and which build of it) is running it, for adapting to the host, telemetry,
/// and attributing behaviour to a concrete build in diagnostics and bug
/// reports.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostInfo {
	/// Platform category the host runs on.
	pub platform: HostPlatform,
	/// Human-readable name of the host implementation, e.g. `"Polkadot
	/// Desktop"`, `"Polkadot Mobile"`, or `"dotli"`. Hosts should report a
	/// stable, non-empty name.
	pub name: String,
	/// Host-native version string, e.g. a semver such as `"1.2.3"`. Hosts
	/// should report a non-empty value. The format is the host's own.
	pub version: String,
	/// Version of the TrUAPI release whose runtime the host embeds, e.g.
	/// `"0.25.0"`.
	///
	/// Two builds of one host can embed different runtimes, so this is what
	/// attributes a protocol behaviour to a runtime release. Empty when the
	/// host answered over v0.1, which does not carry it.
	pub truapi_version: String,
}
