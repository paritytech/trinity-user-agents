use super::*;

#[test]
fn invalid_activation_preserves_the_active_wallet_and_its_grants() {
    let platform = Arc::new(StubPlatform {
        sign_raw_confirmed: false,
        ..StubPlatform::default()
    });
    let (services, authority) = signing_runtime_with_platform(platform.clone());
    futures::executor::block_on(
        authority.activate_local_session_with_identity(ENTROPY.to_vec(), Some("alice".to_string())),
    )
    .expect("initial activation succeeds");
    let session = authority.current_session().expect("active wallet");
    authority
        .grant_auto_signing(&session, "myapp.dot")
        .expect("AutoSigning is granted to the product");
    let runtime = product_runtime(services, authority.clone());

    let error = futures::executor::block_on(
        authority.activate_local_session_with_identity(vec![0xCD; 17], Some("bob".to_string())),
    )
    .expect_err("invalid BIP-39 entropy cannot replace the wallet");
    assert!(matches!(error, AuthorityError::Unavailable { .. }));

    let HostSignRawResponse::V1(response) = futures::executor::block_on(runtime.sign_raw(
        &CallContext::default(),
        HostSignRawRequest::V1(v01::HostSignRawRequest {
            account: v01::ProductAccountId {
                dot_ns_identifier: "myapp.dot".to_string(),
                derivation_index: v01::DerivationIndex::Index(0),
            },
            payload: v01::RawPayload::Bytes {
                bytes: b"still active".to_vec(),
            },
        }),
    ))
    .expect("the active wallet still signs under its existing grant");
    let root = derive_root_keypair_from_entropy(&ENTROPY).expect("original root derives");
    let keypair =
        derive_product_keypair(&root, "myapp.dot", index_bytes(0)).expect("product key derives");
    let signature =
        schnorrkel::Signature::from_bytes(&response.signature).expect("64-byte signature");

    assert_eq!(
        (
            authority.current_session(),
            keypair
                .public
                .verify_simple(b"substrate", b"<Bytes>still active</Bytes>", &signature)
                .is_ok(),
            platform
                .sign_raw_reviews
                .lock()
                .expect("raw signing review list mutex poisoned")
                .is_empty(),
        ),
        (Some(session), true, true),
        "a failed activation must preserve the session token, signing key and grant",
    );
}
