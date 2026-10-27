//! TLS client configuration shared by every native `wss` dial.

use std::sync::{Arc, OnceLock};

use tokio_rustls::rustls::{ClientConfig, RootCertStore, crypto::ring};

/// The TLS config every native `wss` connection uses, built once. It trusts the
/// bundled webpki roots rather than the operating system's trust store, so
/// certificate checks are the same on every native target.
///
/// The crypto provider is named here rather than taken from the process-wide
/// default, which rustls refuses to pick when a build enables more than one.
pub fn client_config() -> Arc<ClientConfig> {
	static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
	Arc::clone(CONFIG.get_or_init(|| {
		let roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
		Arc::new(
			ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
				.with_safe_default_protocol_versions()
				.expect("ring supports every default protocol version")
				.with_root_certificates(roots)
				.with_no_client_auth(),
		)
	}))
}

#[cfg(test)]
mod tests {
	use tokio_rustls::rustls::crypto::CryptoProvider;

	/// A host that installs its own crypto provider, or links a dependency that
	/// enables a second one, must still be able to dial. Taking the process
	/// default would install `ring` as everyone's default on first use, and
	/// would panic in a build where rustls cannot choose one.
	#[test]
	fn building_the_config_leaves_the_process_default_alone() {
		super::client_config();
		assert!(CryptoProvider::get_default().is_none());
	}
}
