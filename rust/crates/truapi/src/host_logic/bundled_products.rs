//! Products a host ships inside its app, one directory each under a bundle
//! root: `<product id>/worker-manifest.json`, `worker/index.js` and
//! `app/index.html`. The host reads its own bundle; the core decides what is
//! in it and which file a request is served from.

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

/// Read access to a host's bundle, by `/`-separated path.
pub trait BundleFiles {
	/// The names directly inside `dir`, empty when it is not a directory.
	fn list(&self, dir: &str) -> Vec<String>;
	/// Whether a file is at `path`.
	fn exists(&self, path: &str) -> bool;
	/// The UTF-8 text of the file at `path`, if it is one.
	fn read_text(&self, path: &str) -> Option<String>;
}

/// One product shipped in the bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
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

/// The products under `root`, by id: each non-hidden, non-empty directory
/// named by a product id.
pub fn bundled_products(root: &str, files: &(impl BundleFiles + ?Sized)) -> Vec<BundledProduct> {
	let mut products: Vec<BundledProduct> = files
		.list(root)
		.into_iter()
		.filter(|name| !name.starts_with('.'))
		.filter_map(|name| {
			let path = join(root, &name);
			if files.list(&path).is_empty() {
				return None;
			}
			let product_id = name.parse::<ProductId>().ok()?;
			Some(BundledProduct {
				product_id,
				has_worker: files.exists(&join(&path, WORKER_SCRIPT)),
				has_app: files.exists(&join(&path, APP_INDEX)),
				path,
			})
		})
		.collect();
	products.sort_by(|a, b| a.product_id.as_str().cmp(b.product_id.as_str()));
	products
}

/// The funding providers under `root`: each product that ships a Worker
/// whose manifest serves Funding, with that manifest, and whether it ships an
/// App. A manifest that does not parse leaves its product out.
pub fn bundled_funding_providers(
	root: &str,
	files: &(impl BundleFiles + ?Sized),
) -> Vec<FundingProviderEntry> {
	bundled_products(root, files)
		.into_iter()
		.filter(|product| product.has_worker)
		.filter_map(|product| {
			let json = files.read_text(&join(&product.path, WORKER_MANIFEST))?;
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

/// The file under a product's `root` that serves `request_path`: the root's
/// `index.html` for an empty path, the file itself, a directory's
/// `index.html` for a path without an extension, and for a main-frame
/// navigation the root's `index.html`, so an App's own routes load it. `None`
/// for a path that climbs out with `..`, or when nothing matches.
pub fn bundled_asset_path(
	root: &str,
	request_path: &str,
	is_main_frame: bool,
	files: &(impl BundleFiles + ?Sized),
) -> Option<String> {
	let path = request_path.trim_matches('/');
	let index = join(root, "index.html");
	if path.is_empty() {
		return files.exists(&index).then_some(index);
	}
	if path.split('/').any(|segment| segment == "..") {
		return None;
	}
	let exact = join(root, path);
	if files.exists(&exact) {
		return Some(exact);
	}
	let last = path.rsplit('/').next().unwrap_or(path);
	if !last.contains('.') {
		let directory_index = join(&exact, "index.html");
		if files.exists(&directory_index) {
			return Some(directory_index);
		}
	}
	(is_main_frame && files.exists(&index)).then_some(index)
}

/// The MIME type a bundled file is served with, by its extension in any
/// case; `application/octet-stream` for one not listed.
pub fn mime_for(path: &str) -> &'static str {
	let name = path.rsplit('/').next().unwrap_or(path);
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
}

#[cfg(test)]
mod tests;
