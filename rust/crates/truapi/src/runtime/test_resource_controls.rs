impl TestResourceControls {
	/// Whether allocation is answered as granted without performing it.
	#[cfg(feature = "test-host")]
	pub fn grants_allowances_unchecked(&self) -> bool {
		self.grant_allowances_unchecked.load(std::sync::atomic::Ordering::Relaxed)
	}

	/// Whether `resource` is answered as refused.
	#[cfg(feature = "test-host")]
	fn withholds(&self, resource: &v01::AllocatableResource) -> bool {
		let tag = match resource {
			v01::AllocatableResource::StatementStoreAllowance => "StatementStoreAllowance",
			v01::AllocatableResource::BulletinAllowance => "BulletinAllowance",
			v01::AllocatableResource::SmartContractAllowance(_) => "SmartContractAllowance",
			v01::AllocatableResource::AutoSigning => "AutoSigning",
		};
		self.withheld_resources
			.lock()
			.expect("withheld resource mutex poisoned")
			.contains(tag)
	}

	/// Refuse a withheld resource before any allowance for it is derived.
	///
	/// The allowance-key calls allocate on their own, without a product ever
	/// asking for an allocation, so a check that lived only in the allocation
	/// answer would hand the key to the very path the product takes.
	#[cfg(feature = "test-host")]
	fn refuse_withheld(&self, resource: &v01::AllocatableResource) -> Result<(), AuthorityError> {
		if self.withholds(resource) {
			return Err(AuthorityError::Rejected);
		}
		Ok(())
	}
}
