use super::*;

fn import_names(source: &str) -> Vec<(String, Shape)> {
	let item_trait: ItemTrait = syn::parse_str(source).unwrap();
	let prefix = snake_case(&item_trait.ident.to_string());
	item_trait
		.items
		.iter()
		.filter_map(|item| match item {
			TraitItem::Fn(method) => Binding::parse(&prefix, method).unwrap(),
			_ => None,
		})
		.map(|binding| (binding.import_name, binding.shape))
		.collect()
}

#[test]
fn guests_see_public_product_initiated_methods_under_wire_table_names() {
	let names = import_names(
		"pub trait LocalStorage {
                #[wire(id = 0)]
                async fn read(&self, cx: &CallContext, request: ReadRequest)
                    -> Result<ReadResponse, CallError<ReadError>>;
                #[wire(id = 1)]
                async fn subscribe(&self, cx: &CallContext, request: SubscribeRequest)
                    -> Subscription<Item, CallError<SubscribeError>>;
                #[wire(id = 2, internal)]
                async fn authorize(&self, cx: &CallContext, request: AuthorizeRequest)
                    -> Result<AuthorizeResponse, CallError<AuthorizeError>>;
                #[wire(id = 3, host_initiated)]
                async fn render(&self, cx: &CallContext, request: RenderRequest)
                    -> Subscription<Node, CallError<RenderError>>;
            }",
	);

	assert_eq!(
		names,
		[
			("local_storage_read".to_string(), Shape::Request),
			("local_storage_subscribe".to_string(), Shape::Subscription),
		]
	);
}
