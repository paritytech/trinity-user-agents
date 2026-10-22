//! TLS client configuration shared by every native `wss` dial.

use std::sync::{Arc, OnceLock};

use tokio_rustls::rustls::{ClientConfig, RootCertStore};

/// One shared client config; webpki roots rather than the platform verifier so
/// the behaviour is identical on every native target, including a statically
/// linked binary on a system without a certificate store.
pub fn client_config() -> Arc<ClientConfig> {
	static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
	Arc::clone(CONFIG.get_or_init(|| {
		let roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
		Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())
	}))
}
