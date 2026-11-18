use super::*;

fn source(input: &str, dev_servers: bool) -> Option<ProductSource> {
	product_source(input.to_string(), "paseo".to_string(), dev_servers)
}

#[test]
fn a_product_runs_from_a_dev_server_then_dotns() {
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
