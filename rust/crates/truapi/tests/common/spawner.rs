use truapi::subscription::Spawner;

/// Test spawner that matches the current target.
pub fn test_spawner() -> Spawner {
	#[cfg(not(target_arch = "wasm32"))]
	{
		static RUNTIME: std::sync::LazyLock<tokio::runtime::Runtime> =
			std::sync::LazyLock::new(|| {
				tokio::runtime::Builder::new_multi_thread()
					.enable_all()
					.build()
					.expect("test runtime")
			});
		Spawner::new(RUNTIME.handle().clone())
	}
	#[cfg(target_arch = "wasm32")]
	{
		Spawner::default()
	}
}
