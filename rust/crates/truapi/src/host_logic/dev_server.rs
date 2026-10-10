//! Products served from a development server on the developer's own network
//! instead of from dotNS.
//!
//! A simulator reaches that server as `localhost`, an Android emulator as
//! `10.0.2.2`, and a phone by the machine's private address. All of them run
//! the product under the same `localhost[:port]` identifier, the only
//! non-dotNS identifier the core accepts, while the page keeps loading from
//! the address that actually reaches the server.

use core::net::Ipv4Addr;

use url::{Host, Url};

const HTTP_PREFIX: &str = "http://";
const HTTPS_PREFIX: &str = "https://";

/// A product served from a development server.
///
/// A `localhost` identifier holds the core's development wildcard, so hosts
/// produce one only in development builds.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct DevServerProduct {
    /// Identifier the product runs under: `localhost` or `localhost:<port>`.
    pub product_id: String,
    /// `http://<host>[:<port>]` origin the page is loaded from.
    pub origin: String,
}

/// The development product `input` names, or `None` when it is not a
/// development server.
///
/// Accepts `localhost` and loopback or private IPv4 hosts over `http`, with or
/// without the scheme. A public address is never a development server, and
/// `https` is refused because a development server is served in the clear.
pub fn parse_dev_server(input: &str) -> Option<DevServerProduct> {
    let trimmed = input.trim();
    let lowercased = trimmed.to_ascii_lowercase();
    let with_scheme = if lowercased.starts_with(HTTP_PREFIX) {
        trimmed.to_string()
    } else if lowercased.starts_with(HTTPS_PREFIX) || trimmed.contains("://") {
        return None;
    } else {
        format!("{HTTP_PREFIX}{trimmed}")
    };

    let parsed = Url::parse(&with_scheme).ok()?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }
    let is_dev_host = match parsed.host()? {
        Host::Domain(domain) => domain == "localhost",
        Host::Ipv4(address) => is_dev_address(address),
        Host::Ipv6(_) => false,
    };
    if !is_dev_host {
        return None;
    }

    let host = parsed.host_str()?;
    let (product_id, origin) = match parsed.port() {
        Some(port) => (format!("localhost:{port}"), format!("{HTTP_PREFIX}{host}:{port}")),
        None => ("localhost".to_string(), format!("{HTTP_PREFIX}{host}")),
    };
    Some(DevServerProduct { product_id, origin })
}

fn is_dev_address(address: Ipv4Addr) -> bool {
    address.is_loopback() || address.is_private()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{is_localhost_product_identifier, normalize_product_identifier};

    fn product(product_id: &str, origin: &str) -> Option<DevServerProduct> {
        Some(DevServerProduct {
            product_id: product_id.to_string(),
            origin: origin.to_string(),
        })
    }

    #[test]
    fn every_way_a_device_reaches_the_machine_runs_under_one_localhost_identifier() {
        for (input, expected) in [
            ("localhost:3000", product("localhost:3000", "http://localhost:3000")),
            ("http://localhost:3000/", product("localhost:3000", "http://localhost:3000")),
            (
                "  HTTP://LocalHost:3000/route?tab=1#top ",
                product("localhost:3000", "http://localhost:3000"),
            ),
            ("127.0.0.1:5173", product("localhost:5173", "http://127.0.0.1:5173")),
            ("10.0.2.2:3000", product("localhost:3000", "http://10.0.2.2:3000")),
            ("192.168.1.59:3000", product("localhost:3000", "http://192.168.1.59:3000")),
            ("http://172.16.0.4:8080", product("localhost:8080", "http://172.16.0.4:8080")),
            ("172.31.255.1:8080", product("localhost:8080", "http://172.31.255.1:8080")),
            ("localhost", product("localhost", "http://localhost")),
            ("http://192.168.1.59:80", product("localhost", "http://192.168.1.59")),
        ] {
            assert_eq!(parse_dev_server(input), expected, "{input}");
        }
    }

    #[test]
    fn the_identifier_is_one_the_core_runs_a_product_under() {
        for input in ["localhost", "localhost:3000", "10.0.2.2:3000", "192.168.1.59:8081"] {
            let product = parse_dev_server(input).expect(input);
            assert!(is_localhost_product_identifier(&product.product_id), "{input}");
            assert_eq!(
                normalize_product_identifier(&product.product_id).as_deref(),
                Ok(product.product_id.as_str()),
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
            assert_eq!(parse_dev_server(input), None, "{input}");
        }
    }
}
