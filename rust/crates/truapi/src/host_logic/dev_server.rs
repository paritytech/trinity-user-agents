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

use crate::platform::{RuntimeConfigValidationError, normalize_product_identifier};

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
	/// Identifier the product runs under: the normalized `product_id` the
	/// address names, otherwise `localhost` or `localhost:<port>`.
	pub product_id: String,
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
	InvalidProductId(RuntimeConfigValidationError),
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
			Some((_, named)) => {
				normalize_product_identifier(&named).map_err(DevServerError::InvalidProductId)?
			},
			None => localhost_id,
		};
		Ok(Self { product_id, origin, url: parsed.into() })
	}
}

fn is_dev_address(address: Ipv4Addr) -> bool {
	address.is_loopback() || address.is_private()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::platform::is_localhost_product_identifier;

	fn product(
		product_id: &str,
		origin: &str,
		url: &str,
	) -> Result<DevServerProduct, DevServerError> {
		Ok(DevServerProduct {
			product_id: product_id.to_string(),
			origin: origin.to_string(),
			url: url.to_string(),
		})
	}

	#[test]
	fn every_way_a_device_reaches_the_machine_runs_under_one_localhost_identifier() {
		for (input, expected) in [
			(
				"localhost:3000",
				product("localhost:3000", "http://localhost:3000", "http://localhost:3000/"),
			),
			(
				"http://localhost:3000/",
				product("localhost:3000", "http://localhost:3000", "http://localhost:3000/"),
			),
			(
				"  HTTP://LocalHost:3000/route?tab=1#top ",
				product(
					"localhost:3000",
					"http://localhost:3000",
					"http://localhost:3000/route?tab=1#top",
				),
			),
			(
				"127.0.0.1:5173",
				product("localhost:5173", "http://127.0.0.1:5173", "http://127.0.0.1:5173/"),
			),
			(
				"10.0.2.2:3000",
				product("localhost:3000", "http://10.0.2.2:3000", "http://10.0.2.2:3000/"),
			),
			(
				"192.168.1.59:3000",
				product("localhost:3000", "http://192.168.1.59:3000", "http://192.168.1.59:3000/"),
			),
			(
				"http://172.16.0.4:8080",
				product("localhost:8080", "http://172.16.0.4:8080", "http://172.16.0.4:8080/"),
			),
			(
				"172.31.255.1:8080",
				product("localhost:8080", "http://172.31.255.1:8080", "http://172.31.255.1:8080/"),
			),
			("localhost", product("localhost", "http://localhost", "http://localhost/")),
			(
				"http://192.168.1.59:80",
				product("localhost", "http://192.168.1.59", "http://192.168.1.59/"),
			),
		] {
			assert_eq!(input.parse::<DevServerProduct>(), expected, "{input}");
		}
	}

	#[test]
	fn a_named_product_id_runs_the_page_as_that_product_and_stays_in_the_address() {
		// The address is what a host keeps for the page, so the name has to
		// survive in it for a reopened page to run as the same product.
		assert_eq!(
			"192.168.1.59:3000/app?product_id=Humanity.DOT".parse::<DevServerProduct>(),
			product(
				"humanity.dot",
				"http://192.168.1.59:3000",
				"http://192.168.1.59:3000/app?product_id=Humanity.DOT",
			)
		);
	}

	#[test]
	fn the_identifier_is_one_the_core_runs_a_product_under() {
		for input in [
			"localhost",
			"localhost:3000",
			"10.0.2.2:3000",
			"192.168.1.59:8081",
			"localhost:3000?product_id=humanity.dot",
		] {
			let product: DevServerProduct = input.parse().expect(input);
			assert_eq!(
				normalize_product_identifier(&product.product_id).as_deref(),
				Ok(product.product_id.as_str()),
				"{input}"
			);
		}
		let product: DevServerProduct = "10.0.2.2:3000".parse().expect("dev server");
		assert!(is_localhost_product_identifier(&product.product_id));
	}

	#[test]
	fn a_product_id_the_core_would_refuse_is_refused_here() {
		// `localhost.dot` is reserved so a published name can never be read
		// as the development wildcard.
		for input in [
			"localhost:3000?product_id=example.com",
			"localhost:3000?product_id=localhost.dot",
			"localhost:3000?product_id=",
		] {
			assert!(
				matches!(
					input.parse::<DevServerProduct>(),
					Err(DevServerError::InvalidProductId(_))
				),
				"{input}"
			);
		}
	}

	#[test]
	fn a_public_or_encrypted_address_is_not_a_development_server() {
		// Admitting any of these would hand the development wildcard to
		// whoever answers at that address.
		for input in [
			"",
			"https://localhost:3000",
			"8.8.8.8:3000",
			"172.32.0.1:3000",
			"192.169.0.1:3000",
			"example.com:3000",
			"localhost.evil.com:3000",
			"localhost:3000.evil",
			"mytestapp.dot",
			"http://user@localhost:3000",
			"http://user:secret@192.168.1.59:3000",
			"ws://localhost:3000",
			"file:///etc/passwd",
			"http://[::1]:3000",
		] {
			assert_eq!(
				input.parse::<DevServerProduct>(),
				Err(DevServerError::NotADevServer),
				"{input}"
			);
		}
	}
}
