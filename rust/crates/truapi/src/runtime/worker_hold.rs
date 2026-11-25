//! A worker reference held by a core operation or a native modality until it releases it.

use std::sync::{Arc, Weak};

use super::RuntimeServices;
use crate::ProductId;

/// One reference held on a product's worker. Acquiring and dropping it may call
/// into the host, so neither operation should run under a lock.
pub struct WorkerHold {
	services: Weak<RuntimeServices>,
	product: ProductId,
}

impl WorkerHold {
	/// Acquire one reference on the product's worker.
	pub fn acquire(services: &Arc<RuntimeServices>, product: &ProductId) -> Self {
		services.worker_ledger.acquire(product);
		Self { services: Arc::downgrade(services), product: product.clone() }
	}

	/// The product whose worker this holds.
	pub fn product(&self) -> &ProductId {
		&self.product
	}
}

impl Drop for WorkerHold {
	fn drop(&mut self) {
		// Services that are gone took their ledger with them.
		if let Some(services) = self.services.upgrade() {
			services.worker_ledger.release(&self.product);
		}
	}
}
