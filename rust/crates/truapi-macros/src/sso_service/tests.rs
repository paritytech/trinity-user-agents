use super::*;

#[test]
fn service_requires_an_explicit_wire_response() {
	let item = syn::parse_quote! {
		impl Service {
			async fn sign(&self, cx: &SsoRequestContext, request: SignRequest)
				-> Result<Vec<u8>, String> { Ok(vec![]) }
		}
	};
	let error = expand_sso_service(item).unwrap_err();
	assert_eq!(
		error.to_string(),
		"expected `async fn name(&self, cx: &SsoRequestContext, request: <Request>) -> <Response>`"
	);
}
