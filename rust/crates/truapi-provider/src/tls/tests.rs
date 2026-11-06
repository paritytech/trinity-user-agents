use tokio_rustls::rustls::crypto::CryptoProvider;

/// The light client's tunnel uses this config directly, without jsonrpsee
/// installing a default first. Taking the process default there panics in a
/// build with two crypto providers and none installed. With one, it installs
/// that one, which is what this detects.
#[test]
fn building_the_config_leaves_the_process_default_alone() {
	super::client_config();
	assert!(CryptoProvider::get_default().is_none());
}
