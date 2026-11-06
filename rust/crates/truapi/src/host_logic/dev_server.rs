//! Products served from a development server on the developer's own network
//! instead of from dotNS.
//!
//! A simulator reaches that server as `localhost`, an Android emulator as
//! `10.0.2.2`, and a phone by the machine's private address. The product runs
//! under the same `localhost[:port]` identifier whichever address reached the
//! server, or under the product id the address names in its `product_id`
//! query, while the page keeps loading from the address that reached it.

use core::{net::Ipv4Addr, str::FromStr};

use url::{Host, Url};

use crate::platform::{ProductId, ProductIdError};

const HTTP_PREFIX: &str = "http://";
const HTTPS_PREFIX: &str = "https://";

/// Query parameter naming the product a development page runs as.
const PRODUCT_ID_PARAM: &str = "product_id";

/// A product served from a development server.
///
/// A `localhost` identifier holds the core's development wildcard, and a named
/// product id gives the page that product's accounts and storage, so hosts
/// produce one only in development builds.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DevServerProduct {
	/// Identifier the product runs under: the `product_id` the address names,
	/// otherwise `localhost` or `localhost:<port>`.
	pub product_id: ProductId,
	/// `http://<host>[:<port>]` origin the page is served from.
	pub origin: String,
	/// The address to load, with the path and query the developer typed.
	pub url: String,
}

/// Why an address does not open as a development product.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum DevServerError {
	/// Not `localhost` or a loopback or private IPv4 address over `http`.
	#[error(
		"not a development server: use localhost, 10.0.2.2 or a private IPv4 address over http"
	)]
	NotADevServer,
	/// The `product_id` query is not an identifier the core runs a product
	/// under.
	#[error(transparent)]
	InvalidProductId(#[from] ProductIdError),
}

/// Accepts `localhost` and loopback or private IPv4 hosts over `http`, with or
/// without the scheme. A public address is never a development server, and
/// `https` is refused because a development server is served in the clear.
impl FromStr for DevServerProduct {
	type Err = DevServerError;

	fn from_str(input: &str) -> Result<Self, Self::Err> {
		let trimmed = input.trim();
		let lowercased = trimmed.to_ascii_lowercase();
		let with_scheme = if lowercased.starts_with(HTTP_PREFIX) {
			trimmed.to_string()
		} else if lowercased.starts_with(HTTPS_PREFIX) || trimmed.contains("://") {
			return Err(DevServerError::NotADevServer);
		} else {
			format!("{HTTP_PREFIX}{trimmed}")
		};

		let parsed = Url::parse(&with_scheme).map_err(|_| DevServerError::NotADevServer)?;
		if !parsed.username().is_empty() || parsed.password().is_some() {
			return Err(DevServerError::NotADevServer);
		}
		match parsed.host() {
			Some(Host::Domain("localhost")) => {},
			Some(Host::Ipv4(address)) if is_dev_address(address) => {},
			_ => return Err(DevServerError::NotADevServer),
		}

		let host = parsed.host_str().ok_or(DevServerError::NotADevServer)?;
		let (localhost_id, origin) = match parsed.port() {
			Some(port) => (format!("localhost:{port}"), format!("{HTTP_PREFIX}{host}:{port}")),
			None => ("localhost".to_string(), format!("{HTTP_PREFIX}{host}")),
		};
		let product_id = match parsed.query_pairs().find(|(name, _)| name == PRODUCT_ID_PARAM) {
			Some((_, named)) => named.into_owned(),
			None => localhost_id,
		};
		let product_id = product_id.parse::<ProductId>()?;
		Ok(Self { product_id, origin, url: parsed.into() })
	}
}

fn is_dev_address(address: Ipv4Addr) -> bool {
	address.is_loopback() || address.is_private()
}

#[cfg(test)]
mod tests;
