use std::collections::BTreeMap;

use super::*;

/// A bundle of files by path; directories are implied by their contents.
struct Bundle(BTreeMap<String, String>);

impl Bundle {
	fn new(files: &[(&str, &str)]) -> Self {
		Self(files.iter().map(|(path, text)| (path.to_string(), text.to_string())).collect())
	}
}

impl BundleReader for Bundle {
	fn list(&self, dir: String) -> Vec<String> {
		let prefix = format!("{dir}/");
		let mut names: Vec<String> = self
			.0
			.keys()
			.filter_map(|path| path.strip_prefix(&prefix))
			.map(|rest| rest.split('/').next().unwrap_or(rest).to_string())
			.collect();
		names.dedup();
		names
	}

	fn exists(&self, path: String) -> bool {
		self.0.contains_key(&path)
	}

	fn read_text(&self, path: String) -> Option<String> {
		self.0.get(&path).cloned()
	}
}

const FUNDING: &str = r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{"funding":{}}}"#;
const CHAT: &str = r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{"chat":true}}"#;

fn bundle() -> Bundle {
	Bundle::new(&[
		("root/ramp.dot/worker-manifest.json", FUNDING),
		("root/ramp.dot/worker/index.js", ""),
		("root/ramp.dot/app/index.html", ""),
		("root/bank.dot/worker-manifest.json", FUNDING),
		("root/bank.dot/worker/index.js", ""),
		("root/chat.dot/worker-manifest.json", CHAT),
		("root/chat.dot/worker/index.js", ""),
		("root/broken.dot/worker-manifest.json", "{"),
		("root/broken.dot/worker/index.js", ""),
		("root/noworker.dot/worker-manifest.json", FUNDING),
		("root/.hidden.dot/worker-manifest.json", FUNDING),
		("root/Not A Product/x", ""),
	])
}

#[test]
fn bundled_funding_providers_ship_a_funding_worker() {
	let providers = bundled_funding_providers("root".to_string(), Box::new(bundle()));
	assert_eq!(
		providers
			.iter()
			.map(|entry| (entry.product_id.as_str().to_string(), entry.origin, entry.app))
			.collect::<Vec<_>>(),
		vec![
			("bank.dot".to_string(), FundingProviderOrigin::Bundled, FundingProviderApp::Absent),
			("ramp.dot".to_string(), FundingProviderOrigin::Bundled, FundingProviderApp::Shipped),
		]
	);
	assert_eq!(providers[0].worker_manifest.as_deref(), Some(FUNDING));
}

const ROOT_MANIFEST_JSON: &str = r#"{"$v":1,"displayName":"Ramp","description":"Card top-ups","icon":{"cid":"bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy","format":"png"},"trustedProducts":{}}"#;

#[test]
fn bundled_products_are_named_directories_described_by_their_root_manifest() {
	let mut files = bundle();
	files
		.0
		.insert("root/ramp.dot/manifest.json".to_string(), ROOT_MANIFEST_JSON.to_string());
	files.0.insert("root/bank.dot/manifest.json".to_string(), "{".to_string());
	let products = bundled_products("root".to_string(), Box::new(files));
	let described: Vec<_> = products
		.iter()
		.map(|product| {
			(
				product.product_id.as_str(),
				product.display_name.as_deref(),
				product.icon_cid.as_deref(),
			)
		})
		.collect();
	assert_eq!(
		described,
		[
			("bank.dot", None, None),
			("broken.dot", None, None),
			("chat.dot", None, None),
			("noworker.dot", None, None),
			(
				"ramp.dot",
				Some("Ramp"),
				Some("bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy")
			),
		]
	);
}

fn entry(id: &str) -> BundleEntry {
	BundleEntry {
		product_id: ProductId::for_test(id),
		path: format!("root/{id}"),
		has_worker: true,
		has_app: true,
		display_name: None,
		icon_cid: None,
	}
}

fn source(input: &str, dev_servers: bool) -> Option<ProductSource> {
	product_source(input.to_string(), "paseo".to_string(), dev_servers, vec![entry("ramp.dot")])
}

#[test]
fn a_product_runs_from_the_bundle_then_a_dev_server_then_dotns() {
	let bundled = Some(ProductSource::Bundled { entry: entry("ramp.dot") });
	assert_eq!(source("ramp.dot", false), bundled);
	assert_eq!(source("https://app.ramp.dot/settings", false), bundled);
	assert_eq!(source("worker.ramp.dot", false), bundled);

	let dev = "http://10.0.2.2:5173/?product_id=ramp.dot";
	assert_eq!(
		source(dev, true),
		Some(ProductSource::DevServer { product: dev.parse().expect("a dev server") })
	);
	assert_eq!(source(dev, false), None);

	let Some(ProductSource::Published { link }) = source("https://shop.paseo/cart", true) else {
		panic!("a published product");
	};
	assert_eq!(link.canonical_url, "https://shop.paseo/cart");
	assert_eq!(source("https://shop.dot/", false), None);
}

#[test]
fn an_archive_answers_the_file_a_directory_index_or_the_app_for_its_routes() {
	let files = || {
		Box::new(Bundle::new(&[
			("app/index.html", ""),
			("app/main.js", ""),
			("app/docs/index.html", ""),
			("app/v1.2/index.html", ""),
		]))
	};
	let cases = [
		("/", false, Some(("app/index.html", "text/html"))),
		("/main.js", false, Some(("app/main.js", "text/javascript"))),
		("/docs", false, Some(("app/docs/index.html", "text/html"))),
		("/v1.2/docs", false, None),
		("/settings", true, Some(("app/index.html", "text/html"))),
		("/settings", false, None),
		("/missing.js", true, Some(("app/index.html", "text/html"))),
		("/../secret", true, None),
	];
	for (path, main, expected) in cases {
		let found = archive_file("app".to_string(), path.to_string(), main, files());
		let expected = expected
			.map(|(path, mime)| ArchiveFile { path: path.to_string(), mime: mime.to_string() });
		assert_eq!(found, expected, "{path}");
	}
}

#[test]
fn a_single_file_archive_answers_every_path() {
	let found = archive_file(
		"cid".to_string(),
		"/any/route".to_string(),
		false,
		Box::new(Bundle::new(&[("cid", "")])),
	);
	assert_eq!(
		found,
		Some(ArchiveFile { path: "cid".to_string(), mime: "application/octet-stream".to_string() })
	);
}

#[test]
fn mime_types_go_by_extension() {
	let cases = [
		("a/index.HTML", "text/html"),
		("main.mjs", "text/javascript"),
		("x.wasm", "application/wasm"),
		("logo.svg", "image/svg+xml"),
		("font.woff2", "font/woff2"),
		("README", "application/octet-stream"),
		("a.b/file", "application/octet-stream"),
	];
	for (path, mime) in cases {
		assert_eq!(mime_for(path), mime, "{path}");
	}
}
