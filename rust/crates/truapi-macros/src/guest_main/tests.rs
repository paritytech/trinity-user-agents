use super::*;

fn error(source: &str) -> Option<String> {
	check(&syn::parse_str(source).unwrap()).err().map(|error| error.to_string())
}

#[test]
fn only_an_argumentless_async_fn_can_be_the_entry_point() {
	assert_eq!(
		[
			error("async fn main() -> Result<(), Error> { Ok(()) }"),
			error("fn main() -> Result<(), Error> { Ok(()) }"),
			error("async fn main(product: String) -> Result<(), Error> { Ok(()) }"),
		],
		[
			None,
			Some("the worker entry point must be an `async fn`".to_string()),
			Some("the worker entry point takes no arguments or generics".to_string()),
		]
	);
}
