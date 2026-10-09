//! View-function signature lookup and decoded-value cache of [`Metadata`].

#[derive(Debug, Clone, Copy)]
pub struct ViewFunctionDef {
	pub id: [u8; 32],
	pub inputs: usize,
	pub output_type: u32,
}

/// View-function lookup and value caching on [`Metadata`]. Kept out of
/// `Metadata`'s inherent interface so the cache stays internal to the
/// statement-allowance machinery.
pub trait MetadataViewCache {
	fn view_function(&self, pallet: &str, function: &str) -> Option<ViewFunctionDef>;

	#[cfg(test)]
	fn insert_view_function(&mut self, pallet: &str, function: &str, definition: ViewFunctionDef);

	fn cached_view_u32(&self, id: &[u8; 32]) -> Option<u32>;

	fn cache_view_u32(&self, id: [u8; 32], value: u32);
}
