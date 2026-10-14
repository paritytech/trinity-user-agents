#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

/// Test spawner that matches the current target.
pub fn test_spawner() -> truapi::subscription::Spawner {
	#[cfg(not(target_arch = "wasm32"))]
	{
		truapi::subscription::thread_per_subscription_spawner()
	}
	#[cfg(target_arch = "wasm32")]
	{
		Arc::new(futures::executor::block_on)
	}
}
