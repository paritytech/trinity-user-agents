//! Executable names within a product's dotNS namespace.

use crate::{
	latest::{ProductId, ProductIdError},
	platform::ProductExecutionKind,
};

impl ProductExecutionKind {
	/// The dotNS prefix used for this kind's executable records.
	pub fn dotns_label(self) -> &'static str {
		match self {
			Self::App => "app",
			Self::Widget => "widget",
			Self::Worker => "worker",
		}
	}
}

impl ProductId {
	/// Name an executable without changing its product's nested namespace.
	pub fn executable(&self, kind: ProductExecutionKind) -> Result<Self, ProductIdError> {
		format!("{}.{self}", kind.dotns_label()).parse()
	}

	/// Remove one executable prefix while keeping a complete, valid product name.
	pub fn executable_owner(&self) -> Result<Self, ProductIdError> {
		if let Some((prefix, owner)) = self.as_str().split_once('.') {
			let is_executable = [
				ProductExecutionKind::App,
				ProductExecutionKind::Widget,
				ProductExecutionKind::Worker,
			]
			.iter()
			.any(|kind| kind.dotns_label() == prefix);
			if is_executable && owner.contains('.') {
				return owner.parse();
			}
		}
		Ok(self.clone())
	}
}

#[cfg(test)]
mod tests {
	use crate::{latest::ProductId, platform::ProductExecutionKind};

	#[test]
	fn executable_owners_preserve_nested_product_names() {
		let owner = ProductId::for_test("arena.coinflip.paseo");
		for (kind, expected) in [
			(ProductExecutionKind::App, "app.arena.coinflip.paseo"),
			(ProductExecutionKind::Widget, "widget.arena.coinflip.paseo"),
			(ProductExecutionKind::Worker, "worker.arena.coinflip.paseo"),
		] {
			let executable = owner.executable(kind).unwrap();
			assert_eq!(executable, ProductId::for_test(expected));
			assert_eq!(executable.executable_owner().unwrap(), owner);
		}
	}

	#[test]
	fn only_an_executable_prefix_with_a_complete_owner_is_removed() {
		for (input, expected) in [
			("app.dot", "app.dot"),
			("arena.coinflip.dot", "arena.coinflip.dot"),
			("app.worker.coinflip.dot", "worker.coinflip.dot"),
			("localhost:3000", "localhost:3000"),
		] {
			assert_eq!(
				ProductId::for_test(input).executable_owner().unwrap(),
				ProductId::for_test(expected)
			);
		}
	}

	#[test]
	fn executable_names_cannot_create_invalid_product_ids() {
		assert!(ProductId::for_test("app.localhost.dot").executable_owner().is_err());
		assert!(
			ProductId::for_test("localhost:3000")
				.executable(ProductExecutionKind::App)
				.is_err()
		);
		let label = "a".repeat(63);
		let longest =
			ProductId::for_test(&format!("{label}.{label}.{label}.{}.dot", "b".repeat(60)));
		assert!(longest.executable(ProductExecutionKind::Worker).is_err());
	}
}
