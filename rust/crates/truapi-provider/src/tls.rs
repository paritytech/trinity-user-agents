//! TLS client configuration shared by every native `wss` dial.

use std::sync::{Arc, OnceLock};

use tokio_rustls::rustls::{ClientConfig, RootCertStore};

/// The TLS config every native `wss` connection uses, built once. It trusts the
/// bundled webpki roots rather than the operating system's trust store, so
/// certificate checks are the same on every native target.
pub fn client_config() -> Arc<ClientConfig> {
	static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
	Arc::clone(CONFIG.get_or_init(|| {
		let roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
		Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())
	}))
}
