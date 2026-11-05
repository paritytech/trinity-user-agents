use super::*;

fn product(product_id: &str, origin: &str, url: &str) -> Result<DevServerProduct, DevServerError> {
	Ok(DevServerProduct {
		product_id: ProductId::for_test(product_id),
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
fn a_product_id_the_core_would_refuse_is_refused_here() {
	// `localhost.dot` is reserved so a published name can never be read
	// as the development wildcard.
	for input in [
		"localhost:3000?product_id=example.com",
		"localhost:3000?product_id=localhost.dot",
		"localhost:3000?product_id=",
	] {
		assert!(
			matches!(input.parse::<DevServerProduct>(), Err(DevServerError::InvalidProductId(_))),
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
