//! Where a product opened by URL or id runs from.

use crate::host_logic::{dev_server::DevServerProduct, dotns::ProductLink};

/// Where a product opened by URL or id runs from.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ProductSource {
	/// Published on dotNS and served from its archive.
	Published {
		/// The product link, with its canonical URL.
		link: ProductLink,
	},
	/// Served from a development server.
	DevServer {
		/// The development product.
		product: DevServerProduct,
	},
}

/// Where the product `url_or_id` names runs from, decided once when a page or
/// Worker opens.
///
/// A development server address when `dev_servers` allows it, then a product
/// link on `network_suffix`. `None` when it is neither.
#[uniffi::export]
pub fn product_source(
	url_or_id: String,
	network_suffix: String,
	dev_servers: bool,
) -> Option<ProductSource> {
	if dev_servers && let Ok(product) = url_or_id.parse::<DevServerProduct>() {
		return Some(ProductSource::DevServer { product });
	}
	ProductLink::parse(&url_or_id, &network_suffix).map(|link| ProductSource::Published { link })
}

#[cfg(test)]
mod tests;
