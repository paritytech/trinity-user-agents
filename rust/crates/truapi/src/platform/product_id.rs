//! The validated product identifier.

use super::{RuntimeConfigValidationError, normalize_product_identifier};

/// A product identifier in the normalized form host-spec C.7 accepts. The only
/// way to build one is [`ProductId::parse`], so an empty or malformed id never
/// reaches code that takes this type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProductId(String);

impl ProductId {
	/// Validates and normalizes `product_id` with [`normalize_product_identifier`].
	pub fn parse(product_id: &str) -> Result<Self, RuntimeConfigValidationError> {
		normalize_product_identifier(product_id).map(Self)
	}

	/// The normalized identifier.
	pub fn as_str(&self) -> &str {
		&self.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// An empty id would share the empty purse tag with Root, among other
	/// collisions, so it never becomes a `ProductId`.
	#[test]
	fn an_empty_id_is_refused() {
		assert_eq!(
			ProductId::parse(""),
			Err(RuntimeConfigValidationError::EmptyField { field: "product_id" })
		);
	}

	#[test]
	fn an_id_is_normalized() {
		assert_eq!(ProductId::parse(" Tickets.DOT ").map(|id| id.0), Ok("tickets.dot".to_owned()));
	}
}
