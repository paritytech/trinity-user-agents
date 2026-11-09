use std::collections::BTreeMap;

use super::*;

/// A bundle of files by path; directories are implied by their contents.
struct Bundle(BTreeMap<String, String>);

impl Bundle {
	fn new(files: &[(&str, &str)]) -> Self {
		Self(files.iter().map(|(path, text)| (path.to_string(), text.to_string())).collect())
	}
}

impl BundleFiles for Bundle {
	fn list(&self, dir: &str) -> Vec<String> {
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

	fn exists(&self, path: &str) -> bool {
		self.0.contains_key(path)
	}

	fn read_text(&self, path: &str) -> Option<String> {
		self.0.get(path).cloned()
	}
}

const FUNDING: &str = r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{"funding":{}}}"#;
const CHAT: &str =
	r#"{"$v":1,"appVersion":[1,0,0],"kind":"worker","entrypoint":"index.js","includes":{}}"#;

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
	let providers = bundled_funding_providers("root", &bundle());
	assert_eq!(
		providers
			.iter()
			.map(|entry| (entry.product_id.as_str().to_string(), entry.origin, entry.has_app))
			.collect::<Vec<_>>(),
		vec![
			("bank.dot".to_string(), FundingProviderOrigin::Bundled, Some(false)),
			("ramp.dot".to_string(), FundingProviderOrigin::Bundled, Some(true)),
		]
	);
	assert_eq!(providers[0].worker_manifest.as_deref(), Some(FUNDING));
}

#[test]
fn bundled_products_are_named_directories() {
	let ids: Vec<String> = bundled_products("root", &bundle())
		.into_iter()
		.map(|product| product.product_id.as_str().to_string())
		.collect();
	assert_eq!(ids, ["bank.dot", "broken.dot", "chat.dot", "noworker.dot", "ramp.dot"]);
}

#[test]
fn an_asset_path_falls_back_to_the_index_for_app_routes() {
	let files =
		Bundle::new(&[("app/index.html", ""), ("app/main.js", ""), ("app/docs/index.html", "")]);
	let cases = [
		("/", false, Some("app/index.html")),
		("/main.js", false, Some("app/main.js")),
		("/docs", false, Some("app/docs/index.html")),
		("/settings", true, Some("app/index.html")),
		("/settings", false, None),
		("/missing.js", true, Some("app/index.html")),
		("/../secret", true, None),
	];
	for (path, main, expected) in cases {
		assert_eq!(bundled_asset_path("app", path, main, &files).as_deref(), expected, "{path}");
	}
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
