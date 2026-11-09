//! Products a host ships inside its app, one directory each under a bundle
//! root: `<product id>/worker-manifest.json`, `worker/index.js` and
//! `app/index.html`. The host reads its own bundle through a
//! [`BundleReader`]; the core decides what is in it and which file a request
//! is served from.

use tracing::warn;

use crate::{
	host_logic::{
		funding_providers::{FundingProviderEntry, FundingProviderOrigin},
		worker_manifest::WorkerManifest,
	},
	platform::ProductId,
};

/// A product's Worker manifest, relative to its directory.
const WORKER_MANIFEST: &str = "worker-manifest.json";
/// A product's Worker script, relative to its directory.
const WORKER_SCRIPT: &str = "worker/index.js";
/// A product's App entry page, relative to its directory.
const APP_INDEX: &str = "app/index.html";

/// Read access to the products a host bundles in its app, by `/`-separated
/// path under the bundle root.
#[uniffi::export(callback_interface)]
pub trait BundleReader: Send + Sync {
	/// The names directly inside directory `dir`; empty when it is not one.
	fn list(&self, dir: String) -> Vec<String>;
	/// Whether a file is at `path`.
	fn exists(&self, path: String) -> bool;
	/// The UTF-8 text of the file at `path`, if it is one.
	fn read_text(&self, path: String) -> Option<String>;
}

/// One product shipped in the bundle.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BundledProduct {
	/// The product.
	pub product_id: ProductId,
	/// Its directory, under the bundle root.
	pub path: String,
	/// Whether it ships a Worker.
	pub has_worker: bool,
	/// Whether it ships an App.
	pub has_app: bool,
}

/// `root` joined with `rest`.
fn join(root: &str, rest: &str) -> String {
	match root.trim_end_matches('/') {
		"" => rest.to_string(),
		root => format!("{root}/{rest}"),
	}
}

/// The products bundled under `root`, by id: each non-hidden, non-empty
/// directory named by a product id, with whether it ships a Worker and an
/// App.
#[uniffi::export]
pub fn bundled_products(root: String, reader: Box<dyn BundleReader>) -> Vec<BundledProduct> {
	products(&root, reader.as_ref())
}

fn products(root: &str, reader: &dyn BundleReader) -> Vec<BundledProduct> {
	let mut products: Vec<BundledProduct> = reader
		.list(root.to_string())
		.into_iter()
		.filter(|name| !name.starts_with('.'))
		.filter_map(|name| {
			let path = join(root, &name);
			if reader.list(path.clone()).is_empty() {
				return None;
			}
			let product_id = name.parse::<ProductId>().ok()?;
			Some(BundledProduct {
				product_id,
				has_worker: reader.exists(join(&path, WORKER_SCRIPT)),
				has_app: reader.exists(join(&path, APP_INDEX)),
				path,
			})
		})
		.collect();
	products.sort_by(|a, b| a.product_id.as_str().cmp(b.product_id.as_str()));
	products
}

/// The funding providers bundled under `root`, as entries for
/// `set_funding_provider_config`.
///
/// Each is a product shipping a Worker whose manifest serves Funding, with
/// that manifest and whether it ships an App. A manifest that does not
/// parse leaves its product out, logged.
#[uniffi::export]
pub fn bundled_funding_providers(
	root: String,
	reader: Box<dyn BundleReader>,
) -> Vec<FundingProviderEntry> {
	products(&root, reader.as_ref())
		.into_iter()
		.filter(|product| product.has_worker)
		.filter_map(|product| {
			let json = reader.read_text(join(&product.path, WORKER_MANIFEST))?;
			match WorkerManifest::parse(&json) {
				Ok(manifest) if manifest.funding.is_some() => Some(FundingProviderEntry {
					product_id: product.product_id,
					worker_manifest: Some(json),
					origin: FundingProviderOrigin::Bundled,
					has_app: Some(product.has_app),
				}),
				Ok(_) => None,
				Err(error) => {
					warn!(product = %product.product_id, %error, "bundled worker manifest does not parse");
					None
				},
			}
		})
		.collect()
}

/// The file under a bundled product's `root` that serves `request_path`.
///
/// That is the root's `index.html` for an empty path, the file itself, a
/// directory's `index.html` for a path without an extension, and for a
/// main-frame navigation the root's `index.html`, so an App's own routes
/// load it. `None` for a path that climbs out with `..`, or when nothing
/// matches.
#[uniffi::export]
pub fn bundled_asset_path(
	root: String,
	request_path: String,
	is_main_frame: bool,
	reader: Box<dyn BundleReader>,
) -> Option<String> {
	let path = request_path.trim_matches('/');
	let index = join(&root, "index.html");
	if path.is_empty() {
		return reader.exists(index.clone()).then_some(index);
	}
	if path.split('/').any(|segment| segment == "..") {
		return None;
	}
	let exact = join(&root, path);
	if reader.exists(exact.clone()) {
		return Some(exact);
	}
	let last = path.rsplit('/').next().unwrap_or(path);
	if !last.contains('.') {
		let directory_index = join(&exact, "index.html");
		if reader.exists(directory_index.clone()) {
			return Some(directory_index);
		}
	}
	(is_main_frame && reader.exists(index.clone())).then_some(index)
}

/// The MIME type a bundled file at `path` is served with, by its extension
/// in any case; `application/octet-stream` for one not listed.
#[uniffi::export]
pub fn mime_for(path: String) -> String {
	let name = path.rsplit('/').next().unwrap_or(&path);
	let extension = name.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase());
	match extension.as_deref() {
		Some("js" | "mjs") => "text/javascript",
		Some("css") => "text/css",
		Some("html" | "htm") => "text/html",
		Some("json" | "map") => "application/json",
		Some("wasm") => "application/wasm",
		Some("svg") => "image/svg+xml",
		Some("png") => "image/png",
		Some("jpg" | "jpeg") => "image/jpeg",
		Some("gif") => "image/gif",
		Some("webp") => "image/webp",
		Some("ico") => "image/x-icon",
		Some("txt") => "text/plain",
		Some("woff") => "font/woff",
		Some("woff2") => "font/woff2",
		Some("ttf") => "font/ttf",
		Some("otf") => "font/otf",
		_ => "application/octet-stream",
	}
	.to_string()
}

#[cfg(test)]
mod tests;
