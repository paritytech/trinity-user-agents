use super::*;
use crate::{platform::Platform, test_support::StubPlatform};
use std::sync::Arc;

#[test]
fn secret_is_generated_once_and_then_reused() {
	let storage: Arc<dyn Platform> = Arc::new(StubPlatform::default());

	let first =
		futures::executor::block_on(read_or_create_device_encryption_secret(storage.as_ref()))
			.unwrap();
	let second =
		futures::executor::block_on(read_or_create_device_encryption_secret(storage.as_ref()))
			.unwrap();

	// Peers address this device by the matching public key, so regenerating
	// it would strand everyone still holding the old one.
	assert_eq!(first, second);
}
