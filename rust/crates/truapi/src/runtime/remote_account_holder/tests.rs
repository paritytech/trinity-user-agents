use super::*;

#[test]
fn unexpected_resource_reasons_include_only_safe_discriminants() {
	let resource = SsoAllocatedResource::AutoSigning {
		product_root_private_key: [0xA5; 64],
		ring_vrf_domain_entropy: [0x5A; 32],
	};

	let AuthorityError::Unknown { reason } =
		unexpected_resource("statement-store allowance", &resource)
	else {
		panic!("expected an unknown authority error");
	};

	assert_eq!(reason, "Unexpected statement-store allowance response resource: auto-signing");
	assert!(!reason.contains("165, 165"));
}
