use super::*;

const KEY: &str = "0x3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29";
const URL: &str = "https://previewnet.substrate.dev/.well-known/trinity-stack.json";

#[test]
fn the_rfc_link_round_trips() {
	let link = format!(
		"polkadotapp://stack?key={KEY}&url=https%3A%2F%2Fpreviewnet.substrate.dev%2F.well-known%2Ftrinity-stack.json"
	);
	let pointer = Pointer::from_link(&link).unwrap();
	assert_eq!(pointer, Pointer::new(URL, Some(KEY)).unwrap());
	assert_eq!(pointer.to_link(DEFAULT_LINK_BASE), link);
}

#[test]
fn any_scheme_carries_a_pointer_and_extra_parameters_are_ignored() {
	let link = format!("https://app.example/open?utm=x&url={URL}");
	assert_eq!(Pointer::from_link(&link), Pointer::new(URL, None));
}

#[test]
fn a_repeated_parameter_is_refused() {
	let link = format!("polkadotapp://stack?url={URL}&url=https://other.example/");
	assert_eq!(Pointer::from_link(&link).map_err(|error| error.code()), Err("invalid-link"));
}

#[test]
fn same_stack_follows_the_key() {
	let signed = Pointer::new(URL, Some(KEY)).unwrap();
	let moved = Pointer::new("https://elsewhere.example/stack.json", Some(KEY)).unwrap();
	let unsigned = Pointer::new(URL, None).unwrap();
	assert!(signed.same_stack(&moved));
	assert!(!signed.same_stack(&unsigned));
	assert!(unsigned.same_stack(&Pointer::new(URL, None).unwrap()));
	assert_eq!(signed.stack_id(), KEY);
	assert_eq!(unsigned.stack_id(), Pointer::new(&format!("{URL}#x"), None).unwrap().stack_id());
}

#[test]
fn an_unsigned_stack_is_its_url_normalised_without_the_fragment() {
	let unsigned = Pointer::new("https://a.example/~s%2fx.json", None).unwrap();
	for same in ["https://a.example/%7Es%2Fx.json#top", "HTTPS://A.EXAMPLE:443/~s%2Fx.json"] {
		assert!(unsigned.same_stack(&Pointer::new(same, None).unwrap()), "{same}");
	}
	let other = Pointer::new("https://a.example/~s/x.json", None).unwrap();
	assert!(!unsigned.same_stack(&other), "%2F is not a path separator");
}

#[test]
fn a_link_base_may_carry_its_own_query() {
	let pointer = Pointer::new(URL, Some(KEY)).unwrap();
	let link = pointer.to_link("https://app.example/open?ref=qr");
	assert_eq!(Pointer::from_link(&link), Ok(pointer));
}
