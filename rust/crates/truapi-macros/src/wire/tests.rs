use super::*;

#[test]
fn internal_methods_keep_their_wire_id_and_other_flags() {
	let args = syn::parse_str::<WireArgs>("id = 2, host_initiated, internal").unwrap();

	assert_eq!(wire_tags(&args), ["@wire_id=2", "@wire_host_initiated", "@wire_internal"]);
}

#[test]
fn repeated_internal_flag_is_rejected() {
	let error = syn::parse_str::<WireArgs>("id = 2, internal, internal")
		.err()
		.expect("duplicate visibility must be rejected");

	assert_eq!(error.to_string(), "duplicate `internal`");
}

#[test]
fn methods_are_public_unless_marked_internal() {
	let args = syn::parse_str::<WireArgs>("id = 2").unwrap();

	assert_eq!(wire_tags(&args), ["@wire_id=2"]);
}
