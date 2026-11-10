//! Products a host runs from files: the ones it ships inside its app, one
//! directory each under a bundle root (`<product id>/manifest.json`,
//! `worker-manifest.json`, `worker/index.js` and `app/index.html`), and the
//! published archives it has downloaded. The host reads files through a
//! [`BundleReader`]; the core decides where a product comes from, what a
//! bundle holds, and which file answers a request.

use tracing::warn;

use crate::{
	host_internal::product_manifest::ProductBrand,
	host_logic::{
		dev_server::DevServerProduct,
		dotns::ProductLink,
		funding_providers::{FundingProviderEntry, FundingProviderOrigin},
		worker_manifest::WorkerManifest,
	},
	image_loader::ImageReference,
	platform::ProductId,
};

/// A product's root manifest, relative to its directory.
const ROOT_MANIFEST: &str = "manifest.json";
/// A product's Worker manifest, relative to its directory.
const WORKER_MANIFEST: &str = "worker-manifest.json";
/// A product's Worker script, relative to its directory.
const WORKER_SCRIPT: &str = "worker/index.js";
/// A product's App entry page, relative to its directory.
const APP_INDEX: &str = "app/index.html";
/// The page a directory is served by.
const INDEX: &str = "index.html";

/// Read access to files by `/`-separated path: a host's bundled products, or a
/// published archive on disk.
#[uniffi::export(callback_interface)]
pub trait BundleReader: Send + Sync {
	/// The names directly inside directory `dir`; empty when it is not one.
	fn list(&self, dir: String) -> Vec<String>;
	/// Whether a regular file is at `path`: `false` for a directory, so a
	/// directory is served by its `index.html` instead.
	fn exists(&self, path: String) -> bool;
	/// The UTF-8 text of the file at `path`, if it is one.
	fn read_text(&self, path: String) -> Option<String>;
}

/// One product shipped in the bundle.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BundleEntry {
	/// The product.
	pub product_id: ProductId,
	/// Its directory, under the bundle root.
	pub path: String,
	/// Whether it ships a Worker.
	pub has_worker: bool,
	/// Whether it ships an App.
	pub has_app: bool,
	/// The name its root manifest gives it, as for a published product.
	pub display_name: Option<String>,
	/// The Bulletin CID of the icon its root manifest names.
	pub icon_cid: Option<String>,
}

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
	/// Shipped in the app and served from its bundle.
	Bundled {
		/// The bundled product.
		entry: BundleEntry,
	},
}

/// The file that answers a request into an archive.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ArchiveFile {
	/// Its path, for the reader.
	pub path: String,
	/// The MIME type it is served with.
	pub mime: String,
}

/// `root` joined with `rest`.
fn join(root: &str, rest: &str) -> String {
	match root.trim_end_matches('/') {
		"" => rest.to_string(),
		root => format!("{root}/{rest}"),
	}
}

/// Where the product `url_or_id` names runs from, decided once when a page or
/// Worker opens.
///
/// A product in `bundle` runs from it, whatever network its id is on. Then,
/// when `dev_servers` allows it, a development server address. Then a product
/// link on `network_suffix`. `None` when it is none of these.
#[uniffi::export]
pub fn product_source(
	url_or_id: String,
	network_suffix: String,
	dev_servers: bool,
	bundle: Vec<BundleEntry>,
) -> Option<ProductSource> {
	let bundled = bundle.into_iter().find(|entry| {
		let owner = entry
			.product_id
			.network_suffix()
			.and_then(|suffix| ProductLink::parse(&url_or_id, suffix))
			.and_then(|link| link.identifier.executable_owner().ok());
		owner.as_ref() == Some(&entry.product_id)
	});
	if let Some(entry) = bundled {
		return Some(ProductSource::Bundled { entry });
	}
	if dev_servers && let Ok(product) = url_or_id.parse::<DevServerProduct>() {
		return Some(ProductSource::DevServer { product });
	}
	ProductLink::parse(&url_or_id, &network_suffix).map(|link| ProductSource::Published { link })
}

/// The products bundled under `root`, by id: each non-hidden, non-empty
/// directory named by a product id, with what it ships and the name and icon
/// its root manifest gives it, read as [`ProductBrand::parse`] reads them. A
/// root manifest that gives no brand leaves both out, logged.
#[uniffi::export]
pub fn bundled_products(root: String, reader: Box<dyn BundleReader>) -> Vec<BundleEntry> {
	products(&root, reader.as_ref())
}

fn products(root: &str, reader: &dyn BundleReader) -> Vec<BundleEntry> {
	let mut products: Vec<BundleEntry> = reader
		.list(root.to_string())
		.into_iter()
		.filter(|name| !name.starts_with('.'))
		.filter_map(|name| {
			let path = join(root, &name);
			if reader.list(path.clone()).is_empty() {
				return None;
			}
			let product_id = name.parse::<ProductId>().ok()?;
			let brand = reader.read_text(join(&path, ROOT_MANIFEST)).and_then(|json| {
				ProductBrand::parse(&json)
					.inspect_err(
						|error| warn!(product = %product_id, %error, "bundled root manifest gives no brand"),
					)
					.ok()
			});
			let (display_name, icon_cid) = brand
				.map(|brand| (Some(brand.display_name), brand.icon.map(cid_of)))
				.unwrap_or_default();
			Some(BundleEntry {
				product_id,
				has_worker: reader.exists(join(&path, WORKER_SCRIPT)),
				has_app: reader.exists(join(&path, APP_INDEX)),
				path,
				display_name,
				icon_cid,
			})
		})
		.collect();
	products.sort_by(|a, b| a.product_id.as_str().cmp(b.product_id.as_str()));
	products
}

/// The CID an icon is loaded by.
fn cid_of(icon: ImageReference) -> String {
	match icon {
		ImageReference::Bulletin { cid, .. } => cid.to_string(),
	}
}

/// The funding providers bundled under `root`, as entries for
/// `set_funding_provider_config`.
///
/// Each is a product shipping a Worker whose manifest serves Funding, with
/// that manifest, its root manifest and whether it ships an App. A Worker
/// manifest that does not parse leaves its product out, logged.
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
					root_manifest: reader.read_text(join(&product.path, ROOT_MANIFEST)),
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

/// The file in the archive at `root` that answers `request_path`, the same
/// for a bundled App and a published archive.
///
/// A `root` that is itself a file is a single-file archive and answers
/// everything. Otherwise the root's `index.html` for an empty path, the file
/// itself, a directory's `index.html` for a last segment without an
/// extension, and for a main-frame navigation the root's `index.html`, so an
/// App's own routes load it. `None` for a path that climbs out with `..`, or
/// when nothing matches, which the host answers with a 404.
#[uniffi::export]
pub fn archive_file(
	root: String,
	request_path: String,
	is_main_frame: bool,
	reader: Box<dyn BundleReader>,
) -> Option<ArchiveFile> {
	let path = archive_path(&root, &request_path, is_main_frame, reader.as_ref())?;
	Some(ArchiveFile { mime: mime_for(&path).to_string(), path })
}

fn archive_path(
	root: &str,
	request_path: &str,
	is_main_frame: bool,
	reader: &dyn BundleReader,
) -> Option<String> {
	if reader.exists(root.to_string()) {
		return Some(root.to_string());
	}
	let path = request_path.trim_matches('/');
	let index = join(root, INDEX);
	if path.is_empty() {
		return reader.exists(index.clone()).then_some(index);
	}
	if path.split('/').any(|segment| segment == "..") {
		return None;
	}
	let exact = join(root, path);
	if reader.exists(exact.clone()) {
		return Some(exact);
	}
	let last = path.rsplit('/').next().unwrap_or(path);
	if !last.contains('.') {
		let directory_index = join(&exact, INDEX);
		if reader.exists(directory_index.clone()) {
			return Some(directory_index);
		}
	}
	(is_main_frame && reader.exists(index.clone())).then_some(index)
}

/// The MIME type a file at `path` is served with, by its extension in any
/// case; `application/octet-stream` for one not listed.
fn mime_for(path: &str) -> &'static str {
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
