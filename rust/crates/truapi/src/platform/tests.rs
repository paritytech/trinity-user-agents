use super::*;

fn file_with_url(url: &str) -> ChatMessageContent {
	ChatMessageContent::File(ChatFile {
		url: url.to_string(),
		file_name: "report.pdf".to_string(),
		mime_type: "application/pdf".to_string(),
		size_bytes: 1,
		text: None,
	})
}

#[test]
fn a_message_url_is_screened_like_an_icon() {
	// The icon field already rejects these. A file card is fetched or
	// opened the same way, so the same allowlist applies.
	for hostile in [
		"javascript:alert(document.cookie)",
		"file:///etc/passwd",
		"content://com.host.provider/secret",
		"data:image/svg+xml,<svg onload=alert(1)>",
	] {
		assert_eq!(
			validate_chat_message_content(file_with_url(hostile)),
			Err(ChatFieldError::RejectedScheme { field: "url" }),
			"{hostile} must be rejected"
		);
	}

	// Characters a URL parser drops are rejected before it runs, so the
	// string a host renders is the string the scheme check ran against.
	for smuggled in [
		"java\tscript:alert(1)",
		"\u{0}javascript:alert(1)",
		"https:/\t/evil.invalid/x",
		"https://\u{200b}evil.invalid/x",
		"https://example.invalid/\u{202e}gpj.exe",
	] {
		assert_eq!(
			validate_chat_message_content(file_with_url(smuggled)),
			Err(ChatFieldError::UnsafeCharacter { field: "url" }),
			"{smuggled} must be rejected"
		);
	}

	// What reaches the host is what the parser resolved.
	assert_eq!(
		validate_chat_message_content(file_with_url("https://example.invalid")),
		Ok(file_with_url("https://example.invalid/"))
	);
}

#[test]
fn a_file_name_cannot_reverse_its_own_extension() {
	// A bidi override renders `invoice<RLO>gnp.exe` as `invoiceexe.png`
	// on the download affordance the host draws.
	let spoofed = ChatMessageContent::File(ChatFile {
		url: "https://example.invalid/f".to_string(),
		file_name: "invoice\u{202e}gnp.exe".to_string(),
		mime_type: "application/pdf".to_string(),
		size_bytes: 1,
		text: None,
	});
	assert_eq!(
		validate_chat_message_content(spoofed),
		Err(ChatFieldError::UnsafeCharacter { field: "fileName" })
	);
}

#[test]
fn message_content_is_bounded_by_count_and_by_bytes() {
	let too_many = ChatMessageContent::Actions(ChatActions {
		text: None,
		actions: (0..CHAT_ACTIONS_MAX + 1)
			.map(|index| ChatAction { action_id: format!("a{index}"), title: "go".to_string() })
			.collect(),
		layout: truapi::latest::ChatActionLayout::Column,
	});
	assert_eq!(
		validate_chat_message_content(too_many),
		Err(ChatFieldError::TooMany { field: "actions", limit: CHAT_ACTIONS_MAX })
	);

	let too_long = ChatMessageContent::Text { text: "x".repeat(CHAT_BODY_MAX_BYTES + 1) };
	assert_eq!(
		validate_chat_message_content(too_long),
		Err(ChatFieldError::TooLong { field: "text", limit: CHAT_BODY_MAX_BYTES })
	);

	let too_much_media = ChatMessageContent::RichText(ChatRichText {
		text: None,
		media: (0..CHAT_MEDIA_MAX + 1)
			.map(|_| ChatMedia { url: "https://example.invalid/m".to_string() })
			.collect(),
	});
	assert_eq!(
		validate_chat_message_content(too_much_media),
		Err(ChatFieldError::TooMany { field: "media", limit: CHAT_MEDIA_MAX })
	);
}

#[test]
fn a_message_body_carries_the_text_a_person_typed() {
	// A chat message is written text: line breaks and tabs are content,
	// and the bytes must survive so a product can echo-compare them.
	let markdown = "# Report\n\n| a | b |\n| - | - |\n\tindented\n";
	assert_eq!(
		validate_chat_message_content(ChatMessageContent::Text { text: markdown.to_string() }),
		Ok(ChatMessageContent::Text { text: markdown.to_string() })
	);

	// Neither trimmed nor NFC-normalized.
	let unnormalized = "  cafe\u{301}  ";
	assert_eq!(
		validate_chat_message_content(ChatMessageContent::Text { text: unnormalized.to_string() }),
		Ok(ChatMessageContent::Text { text: unnormalized.to_string() })
	);

	// The bidi and zero-width screen still applies.
	assert_eq!(
		validate_chat_message_content(ChatMessageContent::Text {
			text: "pay \u{202e}yletamitigel".to_string(),
		}),
		Err(ChatFieldError::UnsafeCharacter { field: "text" })
	);
}

#[test]
fn a_reaction_keeps_the_emoji_a_person_picked() {
	// Subdivision flags encode as tag characters, which a display label
	// rejects and a picked glyph must not.
	for emoji in [
		"\u{1f3f4}\u{e0067}\u{e0062}\u{e0073}\u{e0063}\u{e0074}\u{e007f}",
		"\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}",
		"\u{2764}\u{fe0f}",
		"\u{1f1ee}\u{1f1f3}",
	] {
		assert_eq!(
			validate_chat_message_content(ChatMessageContent::Reaction(ChatReaction {
				message_id: "message-1".to_string(),
				emoji: emoji.to_string(),
			})),
			Ok(ChatMessageContent::Reaction(ChatReaction {
				message_id: "message-1".to_string(),
				emoji: emoji.to_string(),
			})),
			"{emoji:?} must survive"
		);
	}
}

#[test]
fn every_screened_field_rejects_its_own_hostile_value() {
	// One case per screen, so removing any single one fails a test.
	let media = ChatMessageContent::RichText(ChatRichText {
		text: None,
		media: vec![ChatMedia { url: "javascript:alert(1)".to_string() }],
	});
	assert_eq!(
		validate_chat_message_content(media),
		Err(ChatFieldError::RejectedScheme { field: "url" })
	);

	let action = ChatMessageContent::Actions(ChatActions {
		text: None,
		actions: vec![ChatAction {
			action_id: "app\u{200d}rove".to_string(),
			title: "Approve".to_string(),
		}],
		layout: truapi::latest::ChatActionLayout::Column,
	});
	assert_eq!(
		validate_chat_message_content(action),
		Err(ChatFieldError::UnsafeCharacter { field: "actionId" })
	);

	let mime = ChatMessageContent::File(ChatFile {
		url: "https://example.invalid/f".to_string(),
		file_name: "f".to_string(),
		mime_type: "text/\u{202e}nialp".to_string(),
		size_bytes: 1,
		text: None,
	});
	assert_eq!(
		validate_chat_message_content(mime),
		Err(ChatFieldError::UnsafeCharacter { field: "mimeType" })
	);

	let custom_type = ChatMessageContent::Custom(ChatCustomMessage {
		message_type: "  ".to_string(),
		payload: Vec::new(),
	});
	assert_eq!(
		validate_chat_message_content(custom_type),
		Err(ChatFieldError::Empty { field: "messageType" })
	);

	let payload = ChatMessageContent::Custom(ChatCustomMessage {
		message_type: "vote".to_string(),
		payload: vec![0; CHAT_CUSTOM_PAYLOAD_MAX_BYTES + 1],
	});
	assert_eq!(
		validate_chat_message_content(payload),
		Err(ChatFieldError::TooLong { field: "payload", limit: CHAT_CUSTOM_PAYLOAD_MAX_BYTES })
	);

	let emoji = ChatMessageContent::Reaction(ChatReaction {
		message_id: "message-1".to_string(),
		emoji: "\u{202e}".to_string(),
	});
	assert_eq!(
		validate_chat_message_content(emoji),
		Err(ChatFieldError::UnsafeCharacter { field: "emoji" })
	);

	// The removal variant screens the same fields as the addition.
	let removed = ChatMessageContent::ReactionRemoved(ChatReaction {
		message_id: "  ".to_string(),
		emoji: "\u{1f3b2}".to_string(),
	});
	assert_eq!(
		validate_chat_message_content(removed),
		Err(ChatFieldError::Empty { field: "messageId" })
	);

	// Every optional body, not just the one `Text` carries.
	let bidi = "pay \u{202e}yletamitigel".to_string();
	let bodies = [
		ChatMessageContent::RichText(ChatRichText { text: Some(bidi.clone()), media: Vec::new() }),
		ChatMessageContent::Actions(ChatActions {
			text: Some(bidi.clone()),
			actions: Vec::new(),
			layout: truapi::latest::ChatActionLayout::Column,
		}),
		ChatMessageContent::File(ChatFile {
			url: "https://example.invalid/f".to_string(),
			file_name: "f".to_string(),
			mime_type: "text/plain".to_string(),
			size_bytes: 1,
			text: Some(bidi),
		}),
	];
	for body in bodies {
		assert_eq!(
			validate_chat_message_content(body.clone()),
			Err(ChatFieldError::UnsafeCharacter { field: "text" }),
			"{body:?} must screen its body"
		);
	}

	let title = ChatMessageContent::Actions(ChatActions {
		text: None,
		actions: vec![ChatAction {
			action_id: "approve".to_string(),
			title: "Approve\u{202e}".to_string(),
		}],
		layout: truapi::latest::ChatActionLayout::Column,
	});
	assert_eq!(
		validate_chat_message_content(title),
		Err(ChatFieldError::UnsafeCharacter { field: "title" })
	);
}

#[test]
fn an_icon_is_screened_and_resolved_like_a_message_url() {
	// `validate_chat_icon` shares the url path's screen and resolution, and
	// is reached from `create_room` and `register_bot` rather than here.
	assert_eq!(
		validate_chat_icon("icon", "https://example.invalid/\u{202e}gpj.exe"),
		Err(ChatFieldError::UnsafeCharacter { field: "icon" })
	);
	assert_eq!(
		validate_chat_icon("icon", "https://example.invalid").unwrap(),
		"https://example.invalid/"
	);
	assert_eq!(validate_chat_icon("icon", "  ").unwrap(), "");
}

#[test]
fn a_name_cannot_break_its_own_line() {
	// U+2028 and U+2029 are Zl/Zp, not Cc, so `char::is_control` misses
	// them -- yet they break a line exactly like the `\n` this rejects,
	// which is what hides an extension on a one-line download affordance.
	for separator in ['\u{2028}', '\u{2029}'] {
		let spoofed = ChatMessageContent::File(ChatFile {
			url: "https://example.invalid/f".to_string(),
			file_name: format!("invoice.pdf{separator}        .exe"),
			mime_type: "application/pdf".to_string(),
			size_bytes: 1,
			text: None,
		});
		assert_eq!(
			validate_chat_message_content(spoofed),
			Err(ChatFieldError::UnsafeCharacter { field: "fileName" }),
			"{separator:?} must be rejected in a name"
		);
	}
}

#[test]
fn a_url_budget_applies_to_what_the_host_receives() {
	// Resolution percent-encodes, so a URL measured on arrival can land
	// nearly three times over budget.
	// Each of these is 3 bytes raw and 9 percent-encoded, so the arriving
	// string fits the budget and the resolved one does not.
	let padded = format!("https://example.invalid/{}", "\u{4e00}".repeat(300));
	assert!(padded.len() <= CHAT_URL_MAX_BYTES);
	assert_eq!(
		validate_chat_message_content(file_with_url(&padded)),
		Err(ChatFieldError::TooLong { field: "url", limit: CHAT_URL_MAX_BYTES })
	);
}

#[test]
fn action_ids_that_normalize_alike_are_rejected() {
	// Normalization maps these onto one key. Accepting both would give the
	// user two buttons whose trigger the product cannot tell apart.
	let colliding = ChatMessageContent::Actions(ChatActions {
		text: None,
		actions: vec![
			ChatAction { action_id: "approve".to_string(), title: "Approve".to_string() },
			ChatAction { action_id: " approve ".to_string(), title: "Reject".to_string() },
		],
		layout: truapi::latest::ChatActionLayout::Column,
	});
	assert_eq!(
		validate_chat_message_content(colliding),
		Err(ChatFieldError::Duplicate { field: "actionId" })
	);

	// A literal repeat is the same defect without the normalization step.
	let repeated = ChatMessageContent::Actions(ChatActions {
		text: None,
		actions: vec![
			ChatAction { action_id: "approve".to_string(), title: "Approve".to_string() },
			ChatAction { action_id: "approve".to_string(), title: "Reject".to_string() },
		],
		layout: truapi::latest::ChatActionLayout::Column,
	});
	assert_eq!(
		validate_chat_message_content(repeated),
		Err(ChatFieldError::Duplicate { field: "actionId" })
	);
}

#[test]
fn a_file_name_cannot_address_a_path() {
	// A host joining this onto a cache directory must not be handed a
	// separator or a parent reference.
	for traversal in [
		"../../../../data/data/io.parity.wallet/files/session.json",
		"..",
		"a/b.txt",
		"a\\b.txt",
		"C:evil.exe",
	] {
		assert_eq!(
			validate_chat_message_content(ChatMessageContent::File(ChatFile {
				url: "https://example.invalid/f".to_string(),
				file_name: traversal.to_string(),
				mime_type: "application/pdf".to_string(),
				size_bytes: 1,
				text: None,
			})),
			Err(ChatFieldError::PathComponent { field: "fileName" }),
			"{traversal:?} must be rejected"
		);
	}
}

#[test]
fn a_url_budget_is_measured_after_resolution_on_every_field() {
	// Percent-encoding grows a non-ASCII path threefold, so a value that
	// arrives inside its cap can leave well past it. Measuring the arriving
	// string alone let an icon through at three times its budget.
	let long_path = "\u{00e9}".repeat(CHAT_ICON_MAX_BYTES / 4);
	let icon = format!("https://icons.invalid/{long_path}");
	assert!(icon.len() <= CHAT_ICON_MAX_BYTES, "arrives inside its cap");
	assert!(
		Url::parse(&icon).expect("a parsable https url").to_string().len() > CHAT_ICON_MAX_BYTES,
		"resolves past it"
	);

	assert_eq!(
		validate_chat_icon("icon", &icon),
		Err(ChatFieldError::TooLong { field: "icon", limit: CHAT_ICON_MAX_BYTES })
	);

	let message_url =
		format!("https://files.invalid/{}", "\u{00e9}".repeat(CHAT_URL_MAX_BYTES / 4));
	assert!(message_url.len() <= CHAT_URL_MAX_BYTES);
	assert_eq!(
		validate_chat_url("url", &message_url),
		Err(ChatFieldError::TooLong { field: "url", limit: CHAT_URL_MAX_BYTES })
	);
}

#[test]
fn a_url_must_not_carry_credentials() {
	// `Url::to_string` keeps `user:pass@`, so a host handed this would
	// fetch with the credentials and log them.
	for field_url in
		["https://user:pass@example.invalid/avatar.png", "https://user@example.invalid/avatar.png"]
	{
		assert_eq!(
			validate_chat_icon("icon", field_url),
			Err(ChatFieldError::Credentials { field: "icon" }),
			"{field_url:?} must be rejected"
		);
		assert_eq!(
			validate_chat_url("url", field_url),
			Err(ChatFieldError::Credentials { field: "url" }),
			"{field_url:?} must be rejected"
		);
	}
}

#[test]
fn reachability_is_the_hosts_decision_and_the_docs_say_so() {
	// Named rather than incidental: the trait doc tells a host these pass
	// and that fetching them is its own call. A core that guessed would
	// break a host serving its own media from localhost, so if this ever
	// starts rejecting, the doc has to change with it.
	for reachable_only_by_the_host in [
		"https://127.0.0.1:9944/rpc",
		"https://[::1]/admin",
		"https://169.254.169.254/latest/meta-data/",
		"https://10.0.0.1/internal",
	] {
		assert!(
			validate_chat_url("url", reachable_only_by_the_host).is_ok(),
			"{reachable_only_by_the_host:?} is the host's call, not the core's"
		);
	}
}

#[test]
fn the_published_limits_are_the_enforced_limits() {
	// `Chat::post_message`'s doc states these numbers to products in prose,
	// so a test asserting `CONST + 1` would let the constant drift away
	// from the contract without failing.
	assert_eq!(CHAT_BODY_MAX_BYTES, 16 * 1024);
	assert_eq!(CHAT_URL_MAX_BYTES, 2048);
	assert_eq!(CHAT_ACTIONS_MAX, 32);
	assert_eq!(CHAT_MEDIA_MAX, 32);
	assert_eq!(CHAT_CUSTOM_PAYLOAD_MAX_BYTES, 256 * 1024);
	assert_eq!(CHAT_FIELD_MAX_BYTES, 256);
}

#[test]
fn a_reaction_names_its_message_as_an_identifier() {
	// `message_id` addresses a message the way `room_id` addresses a room,
	// so it gets the identifier screen rather than the display one.
	let confusable = ChatMessageContent::Reaction(ChatReaction {
		message_id: "message\u{200d}-1".to_string(),
		emoji: "\u{1f3b2}".to_string(),
	});
	assert_eq!(
		validate_chat_message_content(confusable),
		Err(ChatFieldError::UnsafeCharacter { field: "messageId" })
	);

	let blank = ChatMessageContent::Reaction(ChatReaction {
		message_id: "   ".to_string(),
		emoji: "\u{1f3b2}".to_string(),
	});
	assert_eq!(
		validate_chat_message_content(blank),
		Err(ChatFieldError::Empty { field: "messageId" })
	);
}

#[test]
fn auth_session_storage_key_has_stable_encoding() {
	assert_eq!(CoreStorageKey::AuthSession.encode(), [0]);
}

#[test]
fn sso_responder_request_ledger_key_has_stable_encoding() {
	let key = CoreStorageKey::SsoResponderRequestLedger {
		root_public_key: [0x11; 32],
		peer_statement_account_id: [0x22; 32],
		peer_encryption_public_key: [0x33; 32],
	};
	let mut expected = vec![11];
	expected.extend([0x11; 32]);
	expected.extend([0x22; 32]);
	expected.extend([0x33; 32]);

	assert_eq!(key.encode(), expected);
}

#[test]
fn product_context_encoding_matches_the_generated_host_codec() {
	// The generated TS host codec is
	// `S.Struct({productId: S.str, executionKind: S.Status("App", "Widget", "Worker")})`,
	// so the field order and the variant indices below are the wire
	// contract every JS host decodes against. The JS half of this pair is
	// `product context encoding matches the Rust platform codec` in
	// `js/packages/truapi-host/src/host-callbacks-adapter.test.ts`, which
	// asserts the same bytes through that generated codec.
	assert_eq!(ExecutableKind::App.encode(), [0]);
	assert_eq!(ExecutableKind::Widget.encode(), [1]);
	assert_eq!(ExecutableKind::Worker.encode(), [2]);

	let context =
		ProductContext::new_with_execution(ProductId::for_test("app.dot"), ExecutableKind::Worker);
	assert_eq!(context.encode(), [28, b'a', b'p', b'p', b'.', b'd', b'o', b't', 2]);
	assert_eq!(ProductContext::decode(&mut context.encode().as_slice()), Ok(context));
}

#[test]
fn product_context_decoding_rejects_ids_the_constructor_rejects() {
	// Hand-built frames, not values this crate can encode: `Decode` must
	// apply the same product-id policy as the constructor so decoding
	// cannot mint a context with an unscoped or empty product id.
	for product_id in ["evil.com", "", "  "] {
		let frame = (product_id.to_string(), ExecutableKind::App).encode();
		assert!(
			ProductContext::decode(&mut frame.as_slice()).is_err(),
			"{product_id:?} must not decode into a ProductContext"
		);
	}
}

#[test]
fn product_context_decoding_normalizes_the_product_id() {
	// Derivation and product-scoped storage are keyed by `product_id`, so
	// a non-canonical id off the wire has to land in the same scope the
	// constructor would produce rather than opening a second one.
	let frame = ("App.DOT".to_string(), ExecutableKind::App).encode();
	let decoded = ProductContext::decode(&mut frame.as_slice()).expect("product id normalizes");
	assert_eq!(decoded.product_id.as_str(), "app.dot");
	assert_eq!(decoded, ProductContext::new(ProductId::for_test("app.dot")));
}

/// `slot` of the stack `stack`, encoded as a host receives it.
fn stack_key(slot: CoreStorageKey) -> Vec<u8> {
	StackStorageKey { stack: "stack".to_string(), slot }.encode()
}

#[test]
fn core_storage_key_description_is_strict_and_product_scoped() {
	let permission = stack_key(CoreStorageKey::device_permission_authorization(
		&ProductId::for_test("product.dot"),
		&HostDevicePermissionRequest::Camera,
	));
	assert_eq!(
		describe_core_storage_key(&permission),
		Ok(CoreStorageKeyDescription {
			stack: "stack".to_string(),
			kind: "PermissionAuthorization",
			owner: Some(CoreStorageKeyOwner::Product(ProductId::for_test("product.dot"))),
		})
	);
	for (key, kind, owner) in [
		(CoreStorageKey::AuthSession, "AuthSession", None),
		(CoreStorageKey::PairingDeviceIdentity, "PairingDeviceIdentity", None),
		(
			CoreStorageKey::AllowanceKeys { session_id: "session".to_string() },
			"AllowanceKeys",
			None,
		),
		(CoreStorageKey::LastProcessedPairingStatement, "LastProcessedPairingStatement", None),
		(
			CoreStorageKey::AutoSigningKey { product_id: ProductId::for_test("product.dot") },
			"AutoSigningKey",
			Some(CoreStorageKeyOwner::Product(ProductId::for_test("product.dot"))),
		),
		(CoreStorageKey::AutoSigningKeys, "AutoSigningKeys", None),
		(CoreStorageKey::RingVrfRegistry { root_public_key: [0x42; 32] }, "RingVrfRegistry", None),
		(CoreStorageKey::StatementRenewalTargets, "StatementRenewalTargets", None),
		(CoreStorageKey::DeviceEncryptionKey, "DeviceEncryptionKey", None),
		(
			CoreStorageKey::SsoResponderRequestLedger {
				root_public_key: [0x11; 32],
				peer_statement_account_id: [0x22; 32],
				peer_encryption_public_key: [0x33; 32],
			},
			"SsoResponderRequestLedger",
			None,
		),
		(
			CoreStorageKey::WorkerManifest { product_label: "ramp".to_string() },
			"WorkerManifest",
			Some(CoreStorageKeyOwner::ProductLabel("ramp".to_string())),
		),
		(CoreStorageKey::FundingSupport, "FundingSupport", None),
	] {
		let description = describe_core_storage_key(&stack_key(key)).expect("valid key");
		assert_eq!(
			description,
			CoreStorageKeyDescription { stack: "stack".to_string(), kind, owner }
		);
	}

	assert_eq!(
		describe_core_storage_key(&[]),
		Err(CoreStorageKeyDescriptionError::InvalidEncoding)
	);
	// A slot with no stack is not a key: a host never stores one.
	assert_eq!(
		describe_core_storage_key(&CoreStorageKey::AuthSession.encode()),
		Err(CoreStorageKeyDescriptionError::InvalidEncoding)
	);
	let mut trailing = stack_key(CoreStorageKey::AuthSession);
	trailing.push(0);
	assert_eq!(
		describe_core_storage_key(&trailing),
		Err(CoreStorageKeyDescriptionError::TrailingBytes)
	);
	assert_eq!(
		describe_core_storage_key(&[u8::MAX]),
		Err(CoreStorageKeyDescriptionError::InvalidEncoding)
	);
}

#[test]
fn persisted_permission_and_manifest_scopes_keep_their_string_encoding() {
	let caller = ProductId::for_test("app.caller.dot");
	let target = ProductId::for_test("worker.target.dot");
	let cases = [
		(
			CoreStorageKey::account_access_authorization(&caller, &target),
			(2u8, "app.caller.dot", 3u8, "worker.target.dot").encode(),
			"PermissionAuthorization",
			CoreStorageKeyOwner::Product(caller.clone()),
		),
		(
			CoreStorageKey::product_account_access_authorization(&caller, &target),
			(2u8, "caller", 3u8, "target").encode(),
			"PermissionAuthorization",
			CoreStorageKeyOwner::ProductLabel("caller".to_string()),
		),
		(
			CoreStorageKey::product_signature_authorization(
				&caller,
				&target,
				CrossProductSignatureKind::Raw,
			),
			(2u8, "caller", 4u8, "target", 2u8).encode(),
			"PermissionAuthorization",
			CoreStorageKeyOwner::ProductLabel("caller".to_string()),
		),
		(
			CoreStorageKey::product_manifest(&target),
			(12u8, "target").encode(),
			"ProductManifest",
			CoreStorageKeyOwner::ProductLabel("target".to_string()),
		),
	];
	for (key, encoded, kind, owner) in cases {
		assert_eq!(key.encode(), encoded);
		assert_eq!(CoreStorageKey::decode(&mut encoded.as_slice()), Ok(key.clone()));
		assert_eq!(
			describe_core_storage_key(&stack_key(key)),
			Ok(CoreStorageKeyDescription { stack: "stack".to_string(), kind, owner: Some(owner) })
		);
	}
}

#[test]
fn permission_authorization_keys_separate_product_and_request_variants() {
	let camera = CoreStorageKey::device_permission_authorization(
		&ProductId::for_test("product.dot"),
		&HostDevicePermissionRequest::Camera,
	);
	let other_product = CoreStorageKey::device_permission_authorization(
		&ProductId::for_test("other.dot"),
		&HostDevicePermissionRequest::Camera,
	);
	let remote = CoreStorageKey::remote_permission_authorization(
		&ProductId::for_test("product.dot"),
		&RemotePermissionRequest { permission: RemotePermission::ChainSubmit },
	);
	let identity =
		CoreStorageKey::identity_disclosure_authorization(&ProductId::for_test("product.dot"));
	let other_product_identity =
		CoreStorageKey::identity_disclosure_authorization(&ProductId::for_test("other.dot"));
	let account_access = CoreStorageKey::account_access_authorization(
		&ProductId::for_test("product.dot"),
		&ProductId::for_test("target.dot"),
	);
	let other_target = CoreStorageKey::account_access_authorization(
		&ProductId::for_test("product.dot"),
		&ProductId::for_test("other.dot"),
	);

	assert_ne!(camera, other_product);
	assert_ne!(camera, remote);
	assert_ne!(camera, identity);
	assert_ne!(remote, identity);
	assert_ne!(identity, other_product_identity);
	assert_ne!(account_access, other_target);
	assert_ne!(account_access, camera);
}

#[test]
fn remote_domain_candidates_match_legacy_wildcard_coverage() {
	assert_eq!(
		remote_domain_candidates("api.example.com"),
		["api.example.com", "*.example.com", "*"]
	);
	assert_eq!(
		remote_domain_candidates("deep.api.example.com"),
		["deep.api.example.com", "*.api.example.com", "*.example.com", "*"]
	);
	assert_eq!(remote_domain_candidates("example.com"), ["example.com", "*"]);
	assert_eq!(remote_domain_candidates("wallet.dot"), ["wallet.dot", "*"]);
	// A single-label host has no parent to wildcard over.
	assert_eq!(remote_domain_candidates("localhost"), ["localhost", "*"]);
	// A stored pattern resolves to itself, not to a duplicated entry.
	assert_eq!(remote_domain_candidates("*.example.com"), ["*.example.com", "*"]);
	assert_eq!(remote_domain_candidates("*"), ["*"]);
	assert_eq!(remote_domain_candidates("*.com"), ["*.com", "*"]);
	assert_eq!(
		remote_domain_candidates("*.api.example.com"),
		["*.api.example.com", "*.example.com", "*"]
	);
	assert_eq!(remote_domain_candidates("example.co.uk"), ["example.co.uk", "*.co.uk", "*"]);
	assert_eq!(remote_domain_candidates("127.0.0.1"), ["127.0.0.1", "*"]);
	assert_eq!(remote_domain_candidates("[::1]"), ["[::1]", "*"]);
	assert_eq!(
		remote_domain_candidates("DEEP.API.Bücher.Example."),
		[
			"deep.api.xn--bcher-kva.example",
			"*.api.xn--bcher-kva.example",
			"*.xn--bcher-kva.example",
			"*",
		]
	);
	for (pattern, valid) in [
		("example.com", true),
		("localhost", true),
		("127.0.0.1", true),
		("[::1]", true),
		("*", true),
		("*.example.com", true),
		("*.co.uk", true),
		("*.Bücher.example.", true),
		("*.com", false),
		("*.dot", false),
		("*.127.0.0.1", false),
		("*.0.1", false),
		("*.[::1]", false),
		("*..com", false),
		("api.*.com", false),
		("", false),
	] {
		assert_eq!(is_valid_remote_domain_pattern(pattern), valid, "{pattern}");
	}
}

#[test]
fn remote_domain_normalization_is_shared_by_both_ends_of_a_lookup() {
	// The forms enforcement can hand in from a real URL host all collapse
	// onto the one key a grant is stored under.
	for spelling in ["API.Example.COM", "api.example.com.", "  api.example.com  "] {
		assert_eq!(normalize_remote_domain(spelling), "api.example.com");
	}
	// IDNA: a non-ASCII host and its punycode spelling are one site, so
	// they must be one slot and one prompt.
	assert_eq!(normalize_remote_domain("Bücher.example"), "xn--bcher-kva.example");
	assert_eq!(normalize_remote_domain("xn--bcher-kva.example"), "xn--bcher-kva.example");
	// The wildcard marker is not part of the host and survives untouched.
	assert_eq!(normalize_remote_domain("*.Example.COM"), "*.example.com");
	assert_eq!(normalize_remote_domain("*.Bücher.example"), "*.xn--bcher-kva.example");
	assert_eq!(normalize_remote_domain("*"), "*");
	// A canonicalized bundle keys the same as the candidate list built from
	// a live host, which is what makes the grant visible to enforcement.
	assert_eq!(
		CoreStorageKey::remote_domain_authorization(
			&ProductId::for_test("product.dot"),
			"API.Example.COM."
		),
		CoreStorageKey::remote_domain_authorization(
			&ProductId::for_test("product.dot"),
			&remote_domain_candidates("api.example.com.")[0]
		)
	);
}

#[test]
fn remote_domain_authorization_key_matches_the_one_element_bundle() {
	// Enforcement keys a single host; a product granting that one domain
	// must land in the same slot, or the grant is invisible to the gate.
	assert_eq!(
		CoreStorageKey::remote_domain_authorization(
			&ProductId::for_test("product.dot"),
			"Example.COM"
		),
		CoreStorageKey::remote_permission_authorization(
			&ProductId::for_test("product.dot"),
			&RemotePermissionRequest {
				permission: RemotePermission::Remote { domains: vec!["example.com".to_string()] },
			}
		)
	);
	assert_ne!(
		CoreStorageKey::remote_domain_authorization(
			&ProductId::for_test("product.dot"),
			"example.com"
		),
		CoreStorageKey::remote_domain_authorization(
			&ProductId::for_test("product.dot"),
			"*.example.com"
		)
	);
	assert_ne!(
		CoreStorageKey::remote_domain_authorization(
			&ProductId::for_test("product.dot"),
			"example.com"
		),
		CoreStorageKey::remote_domain_authorization(
			&ProductId::for_test("other.dot"),
			"example.com"
		)
	);
}

#[test]
fn remote_permission_authorization_key_canonicalizes_domain_sets() {
	let unsorted = RemotePermissionRequest {
		permission: RemotePermission::Remote {
			domains: vec!["b.example.com".into(), "a.example.com".into()],
		},
	};
	let sorted = RemotePermissionRequest {
		permission: RemotePermission::Remote {
			domains: vec!["a.example.com".into(), "b.example.com".into()],
		},
	};
	assert_eq!(
		CoreStorageKey::remote_permission_authorization(
			&ProductId::for_test("product.dot"),
			&unsorted
		),
		CoreStorageKey::remote_permission_authorization(
			&ProductId::for_test("product.dot"),
			&sorted
		)
	);

	let mixed = RemotePermissionRequest {
		permission: RemotePermission::Remote {
			domains: vec!["Example.COM".into(), "a.com".into(), "a.com".into()],
		},
	};
	let canonical = RemotePermissionRequest {
		permission: RemotePermission::Remote {
			domains: vec!["a.com".into(), "example.com".into()],
		},
	};
	assert_eq!(
		CoreStorageKey::remote_permission_authorization(
			&ProductId::for_test("product.dot"),
			&mixed
		),
		CoreStorageKey::remote_permission_authorization(
			&ProductId::for_test("product.dot"),
			&canonical
		)
	);
}

#[test]
fn remote_permission_authorization_key_handles_separator_chars_in_domains() {
	let injecting = RemotePermissionRequest {
		permission: RemotePermission::Remote {
			domains: vec!["a|b".into(), "c,d".into(), "remote:web-rtc".into()],
		},
	};
	let benign_same_set = RemotePermissionRequest {
		permission: RemotePermission::Remote { domains: vec!["x".into(), "y".into(), "z".into()] },
	};
	let injecting_key = CoreStorageKey::remote_permission_authorization(
		&ProductId::for_test("product.dot"),
		&injecting,
	);
	let benign_key = CoreStorageKey::remote_permission_authorization(
		&ProductId::for_test("product.dot"),
		&benign_same_set,
	);
	assert_ne!(injecting_key, benign_key);

	let webrtc = RemotePermissionRequest { permission: RemotePermission::WebRtc };
	assert_ne!(
		injecting_key,
		CoreStorageKey::remote_permission_authorization(
			&ProductId::for_test("product.dot"),
			&webrtc
		)
	);

	let injecting_reordered = RemotePermissionRequest {
		permission: RemotePermission::Remote {
			domains: vec!["remote:web-rtc".into(), "c,d".into(), "a|b".into()],
		},
	};
	assert_eq!(
		injecting_key,
		CoreStorageKey::remote_permission_authorization(
			&ProductId::for_test("product.dot"),
			&injecting_reordered
		)
	);
}

/// A host decodes reviews with its own codec. If a discriminant moves, a
/// host one version apart shows the user a different consent prompt from
/// the one its answer approves.
#[test]
fn review_discriminants_do_not_move() {
	let discriminants = [
		UserConfirmationReview::AccountAccess(AccountAccessReview {
			requesting_product_id: ProductId::for_test("caller.dot"),
			target_product_id: ProductId::for_test("target.dot"),
		}),
		UserConfirmationReview::ProductSubtree(ProductSubtreeReview {
			product_id: ProductId::for_test("caller.dot"),
		}),
	]
	.map(|review| review.encode()[0]);

	assert_eq!(discriminants, [9, 11]);
}
