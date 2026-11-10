use super::*;

#[test]
fn product_id_keeps_its_type_without_changing_wire_shape() {
	let product_id = TypeDef {
		name: "ProductId".to_string(),
		module_path: vec![],
		generic_params: vec![],
		kind: TypeDefKind::Alias(TypeRef::Primitive("str".to_string())),
		docs: None,
	};
	let mut typed = api_with_payload_fields(vec![("product_id", named_type("ProductId"))]);
	typed.types.push(product_id);
	let strings =
		api_with_payload_fields(vec![("product_id", TypeRef::Primitive("str".to_string()))]);
	assert_eq!(wire_schema_hash(&typed, 1, 1).unwrap(), wire_schema_hash(&strings, 1, 1).unwrap());
	let emitted = generate_types(&typed, 1).unwrap();
	assert!(emitted.contains("export type ProductId = string &"));
	assert!(emitted.contains("productId: ProductId"));
	assert!(emitted.contains("S.enhanceCodec(S.str, (value: ProductId) => value, asProductId)"));
}

fn unit_variant(name: &str, codec_index: Option<u8>) -> VariantDef {
	VariantDef { name: name.to_string(), fields: VariantFields::Unit, docs: None, codec_index }
}

// A unit-only enum encodes by position, so an index pinned out of
// declaration order would ship the wrong byte; codegen refuses it.
#[test]
fn a_unit_enum_pinned_out_of_order_is_refused() {
	let in_order = [unit_variant("A", Some(0)), unit_variant("B", Some(1))];
	let out_of_order = [unit_variant("A", Some(1)), unit_variant("B", Some(0))];

	assert_eq!(
		unit_enum_codec_expr("Pinned", &in_order).expect("in order"),
		"S.Status(\"A\", \"B\")"
	);
	assert!(unit_enum_codec_expr("Pinned", &out_of_order).is_err());
}

fn wire_attrs(id: Option<u8>) -> WireAttrs {
	WireAttrs { id, ..WireAttrs::default() }
}

/// Build a one-method API whose request payload is `struct Payload`, with the
/// given named fields, so a test can vary only the payload layout.
fn api_with_payload_fields(fields: Vec<(&str, TypeRef)>) -> ApiDefinition {
	let payload = TypeDef {
		name: "Payload".to_string(),
		module_path: Vec::new(),
		generic_params: Vec::new(),
		kind: TypeDefKind::Struct(
			fields
				.into_iter()
				.map(|(name, type_ref)| FieldDef { name: name.to_string(), type_ref, docs: None })
				.collect(),
		),
		docs: None,
	};
	let method = MethodDef {
		name: "do_thing".to_string(),
		kind: MethodKind::Request,
		params: vec![ParamDef {
			name: "request".to_string(),
			type_ref: TypeRef::Named { name: "Payload".to_string(), args: Vec::new() },
		}],
		return_type: ReturnType::Result { ok: TypeRef::Unit, err: TypeRef::Unit },
		wire: wire_attrs(Some(7)),
		docs: None,
	};
	ApiDefinition {
		traits: vec![TraitDef {
			name: "Thing".to_string(),
			module_path: Vec::new(),
			wire_trait_id: Some(200),
			methods: vec![method],
			docs: None,
		}],
		public_trait_order: vec!["Thing".to_string()],
		types: vec![payload],
		framework_types: Vec::new(),
	}
}

fn internal_api_fixture() -> ApiDefinition {
	let mut internal = request_method_with_wrappers(
		"do_thing",
		Some(7),
		"HiddenRequest",
		"SharedResponse",
		"SharedError",
	);
	internal.wire.internal = true;
	internal.docs = Some("```ts\nawait truapi.thing.doThing();\n```".into());
	let mut public = request_method_with_wrappers(
		"read_shared",
		Some(8),
		"PublicRequest",
		"SharedResponse",
		"SharedError",
	);
	public.docs = Some("```ts\nawait truapi.thing.readShared();\n```".into());
	let mut api = api(vec![internal, public]);
	api.traits[0].name = "Thing".into();
	api.public_trait_order = vec!["Thing".into()];
	for name in ["HiddenRequest", "PublicRequest", "SharedResponse", "SharedError"] {
		let inner = format!("V01{name}");
		api.types.push(versioned_tuple_wrapper_variants(name, &[(1, &inner)]));
		let mut payload = empty_struct(&inner);
		payload.kind = TypeDefKind::Struct(vec![FieldDef {
			name: "shared".into(),
			type_ref: named_type("Shared"),
			docs: None,
		}]);
		api.types.push(payload);
	}
	api.types.push(empty_struct("Shared"));
	api
}

#[test]
fn internal_methods_are_absent_from_product_bindings_and_documentation() {
	let api = internal_api_fixture();
	let directory = tempfile::tempdir().unwrap();
	let output = directory.path().to_str().unwrap();
	generate(&api, output, 1, 2).unwrap();
	generate_playground_services(&api, output, 1, false).unwrap();
	generate_explorer(&api, output, 1).unwrap();
	let examples = directory.path().join("examples");
	generate_client_examples(&api, examples.to_str().unwrap(), 1).unwrap();
	let read = |file: &str| fs::read_to_string(directory.path().join(file)).unwrap();
	let client = read("client.ts");
	let types = read("types.ts");
	let services = read("codegen/services.ts");
	let explorer = read("codegen/types.ts");
	assert_eq!(
		(
			client.contains("doThing"),
			types.contains("HiddenRequest"),
			services.contains("do_thing"),
			explorer.contains("HiddenRequest"),
			examples.join("thing-do-thing.ts").exists(),
		),
		(false, false, false, false, false),
	);
	assert_eq!(
		(
			client.contains("readShared"),
			types.contains("export interface Shared"),
			services.contains("read_shared"),
			explorer.contains("Shared"),
			examples.join("thing-read-shared.ts").exists(),
		),
		(true, true, true, true, true),
	);
	let internal = read("internal.ts");
	assert!(internal.contains("export interface HiddenRequest"));
	assert!(internal.contains("import { Shared } from './types.js'"));
	assert_eq!(generate_index(), "export * from './types.js';\nexport * from './client.js';\n");
}

#[test]
fn internal_bindings_keep_typed_calls_separate_and_immutable() {
	let api = internal_api_fixture();
	let directory = tempfile::tempdir().unwrap();
	generate(&api, directory.path().to_str().unwrap(), 1, 2).unwrap();
	let internal = fs::read_to_string(directory.path().join("internal-client.ts")).unwrap();
	assert!(internal.contains("import * as T from './internal.js'"));
	assert!(
		internal.contains("createInternalClient(transport: TrUApiTransport): InternalTrUApiClient")
	);
	assert!(
		internal.contains("return this.#transport.request<T.SharedResponse, T.SharedError>"),
		"{internal}"
	);
	assert!(internal.contains("ids: W.THING_DO_THING"));
	assert!(internal.contains("T.VersionedHiddenRequest.enc"));
	assert!(
		internal
			.contains("S.Result(T.VersionedSharedResponse, T.VersionedSharedError).dec(payload)")
	);
	assert!(internal.contains("signal: options?.signal"));
	assert!(internal.contains("Object.freeze(ThingClient.prototype)"));
	assert!(internal.contains("thing: Object.freeze(new ThingClient(transport))"));
	assert!(internal.contains("return Object.freeze({"));
	assert!(!internal.contains("readShared"));
	assert!(!internal.contains("export class ThingClient"));
	assert!(!internal.contains("createObservable"));
}

#[test]
fn public_clients_hide_shared_transport_without_freezing_the_api() {
	let source = generate_client(&internal_api_fixture(), 1, 2).unwrap();
	assert!(source.contains("readonly #transport: TrUApiTransport"));
	assert!(source.contains("this.#transport = transport"));
	assert!(source.contains("return this.#transport.request"));
	assert!(!source.contains("this.transport"));
	assert!(!source.contains("Object.freeze"));
	assert!(
		source.contains("S.Result(T.VersionedSharedResponse, T.VersionedSharedError).dec(payload)")
	);

	let mut method = request_method_with_wrappers(
		"render",
		Some(0),
		"RenderRequest",
		"RenderItem",
		"RenderError",
	);
	let ReturnType::Result { ok, err } = method.return_type else {
		unreachable!();
	};
	method.kind = MethodKind::Subscription;
	method.return_type = ReturnType::Subscription { item: ok, interrupt: err };
	method.wire.host_initiated = true;
	let mut api = api(vec![method]);
	api.public_trait_order = vec!["Example".to_string()];
	for name in ["RenderRequest", "RenderItem", "RenderError"] {
		let inner = format!("V01{name}");
		api.types.push(versioned_tuple_wrapper_variants(name, &[(1, &inner)]));
		api.types.push(empty_struct(&inner));
	}
	let source = generate_client(&api, 1, 2).unwrap();
	assert!(
		source.contains("readonly #renderRegistration: HostInitiatedSubscriptionRegistration"),
		"{source}"
	);
	assert!(
		source.contains("this.#renderRegistration = transport.registerHostInitiatedSubscription")
	);
	assert!(source.contains("return this.#renderRegistration.setHandler(handler)"));
}

#[test]
fn host_initiated_requests_use_the_subscription_wire_contract() {
	let mut api = internal_api_fixture();
	api.traits[0].methods[1].wire.host_initiated = true;
	let actual = (
		generate_wire_table(&api, 1).unwrap(),
		generate_decode_table(&api, 1).unwrap(),
		wire_schema_hash(&api, 1, 3).unwrap(),
		crate::rust::generate_wire_table(&api, "hash").unwrap(),
	);
	let method = &mut api.traits[0].methods[1];
	let ReturnType::Result { ok, err } = core::mem::replace(
		&mut method.return_type,
		ReturnType::Result { ok: TypeRef::Unit, err: TypeRef::Unit },
	) else {
		unreachable!();
	};
	method.kind = MethodKind::Subscription;
	method.return_type = ReturnType::Subscription { item: ok, interrupt: err };
	assert_eq!(
		actual,
		(
			generate_wire_table(&api, 1).unwrap(),
			generate_decode_table(&api, 1).unwrap(),
			wire_schema_hash(&api, 1, 3).unwrap(),
			crate::rust::generate_wire_table(&api, "hash").unwrap(),
		)
	);
}

#[test]
fn host_initiated_requests_register_result_handlers() {
	let mut api = internal_api_fixture();
	api.traits[0].methods[1].wire.host_initiated = true;
	let source = generate_client(&api, 1, 3).unwrap();
	assert!(
		source.contains(
			"HostInitiatedSubscriptionRegistration<Extract<T.VersionedPublicRequest, { tag: \"V1\" }>[\"value\"], Extract<T.VersionedSharedResponse, { tag: \"V1\" }>[\"value\"], T.SharedError>"
		),
		"{source}"
	);
	assert!(source.contains("transport.registerHostInitiatedSubscription({"), "{source}");
	assert!(source.contains("singleResponse: { requestVersion: 1 },"), "{source}");
	assert!(source.contains("if (request.tag !== \"V1\")"), "{source}");
	assert!(
		source.contains(
			"handler: HostInitiatedRequestHandler<Extract<T.VersionedPublicRequest, { tag: \"V1\" }>[\"value\"], Extract<T.VersionedSharedResponse, { tag: \"V1\" }>[\"value\"], T.SharedError>"
		),
		"{source}"
	);
	assert!(
		source.contains(
			"encodeItem: (item) => T.VersionedSharedResponse.enc({ tag: \"V1\", value: item })"
		),
		"{source}"
	);
	assert!(source.contains("setHandler(hostRequestHandler(handler))"), "{source}");
	assert!(!source.contains("registerHostInitiatedRequest"), "{source}");
	assert!(!source.contains("this.#transport.request"), "{source}");
}

#[test]
fn internal_visibility_preserves_wire_addresses_hash_and_decoding() {
	let mut api = internal_api_fixture();
	let hash = wire_schema_hash(&api, 1, 2).unwrap();
	let table = generate_wire_table(&api, 1).unwrap();
	let decoder = generate_decode_table(&api, 1).unwrap();
	assert!(decoder.contains("W.THING_DO_THING"));
	assert!(decoder.contains("T.VersionedHiddenRequest"));
	assert!(generate_client(&api, 1, 2).unwrap().contains(&hash));
	api.traits[0].methods[0].wire.internal = false;
	assert_eq!(wire_schema_hash(&api, 1, 2).unwrap(), hash);
	assert_eq!(generate_wire_table(&api, 1).unwrap(), table);
	assert_eq!(generate_decode_table(&api, 1).unwrap(), decoder);
}

#[test]
fn internal_versions_do_not_take_public_payload_names() {
	let mut api = internal_api_fixture();
	api.types
		.retain(|ty| !matches!(ty.name.as_str(), "HiddenRequest" | "PublicRequest"));
	api.types.extend([
		versioned_tuple_wrapper_variants("HiddenRequest", &[(1, "V02Payload")]),
		versioned_tuple_wrapper_variants("PublicRequest", &[(1, "V01Payload")]),
		single_field_struct("V01Payload", "public_value", "bool"),
		single_field_struct("V02Payload", "private_value", "bool"),
	]);

	let public = generate_types(&api, 2).unwrap();
	let internal = generate_type_bindings(&api, 2, true).unwrap();
	assert!(public.contains("export interface Payload {\n  publicValue: boolean;"));
	assert!(internal.contains("export interface V02Payload {\n  privateValue: boolean;"));
	assert!(internal.contains("V1: [0, V02Payload]"));
}

#[test]
fn internal_imports_follow_the_selected_wire_version() {
	let mut api = internal_api_fixture();
	let wrapper = api.types.iter_mut().find(|ty| ty.name == "HiddenRequest").unwrap();
	*wrapper =
		versioned_tuple_wrapper_variants("HiddenRequest", &[(1, "Shared"), (2, "FutureShared")]);
	let TypeDefKind::Enum(variants) = &mut wrapper.kind else { unreachable!() };
	variants.push(VariantDef {
		name: "V3".into(),
		fields: VariantFields::Unnamed(vec![TypeRef::Vec(Box::new(TypeRef::Primitive(
			"u8".into(),
		)))]),
		docs: None,
		codec_index: None,
	});
	api.types.push(empty_struct("FutureShared"));
	api.types.push(TypeDef {
		name: "PublicHelper".into(),
		module_path: Vec::new(),
		generic_params: Vec::new(),
		kind: TypeDefKind::Alias(named_type("FutureShared")),
		docs: None,
	});

	let internal = generate_type_bindings(&api, 1, true).unwrap();
	assert!(internal.contains("import { Shared } from './types.js';"));
	assert!(!internal.contains("FutureShared"));
	assert!(!internal.contains("HexString"));
}

#[test]
fn option_bool_fields_are_optional_and_keep_their_compact_codec() {
	let api = api_with_payload_fields(vec![(
		"with_signed_transaction",
		TypeRef::Primitive("optionBool".to_string()),
	)]);

	let source = generate_types(&api, 1).expect("generate types");

	assert!(source.contains("withSignedTransaction?: boolean;"));
	assert!(source.contains("withSignedTransaction: S.OptionBool"));
}

#[test]
fn schema_hash_moves_when_a_payload_field_type_changes() {
	// The drift class this fingerprint exists to catch: same frame ids, same
	// method names, same sensitivity - only a field's width changed. A newer
	// host's bytes would otherwise decode on the old table without throwing,
	// silently yielding wrong values (the shape of the getAccount P0).
	let before = api_with_payload_fields(vec![
		("ring_index", TypeRef::Primitive("u32".to_string())),
		("ring_revision", TypeRef::Primitive("u32".to_string())),
	]);
	let after = api_with_payload_fields(vec![
		("ring_index", TypeRef::Primitive("u64".to_string())),
		("ring_revision", TypeRef::Primitive("u32".to_string())),
	]);

	assert_ne!(wire_schema_hash(&before, 1, 1).unwrap(), wire_schema_hash(&after, 1, 1).unwrap(),);
}

#[test]
fn schema_hash_moves_when_same_width_payload_fields_are_reordered() {
	// Nastier than a width change: the frame length is identical, so no
	// arithmetic check can see it and the decode cannot fail - the values
	// simply swap.
	let before = api_with_payload_fields(vec![
		("ring_index", TypeRef::Primitive("u32".to_string())),
		("ring_revision", TypeRef::Primitive("u32".to_string())),
	]);
	let after = api_with_payload_fields(vec![
		("ring_revision", TypeRef::Primitive("u32".to_string())),
		("ring_index", TypeRef::Primitive("u32".to_string())),
	]);

	assert_ne!(wire_schema_hash(&before, 1, 1).unwrap(), wire_schema_hash(&after, 1, 1).unwrap(),);
}

#[test]
fn schema_hash_is_stable_for_an_unchanged_contract() {
	// The fingerprint must not be noisy: an identical contract hashes
	// identically, or every host would look drifted.
	let api = api_with_payload_fields(vec![("ring_index", TypeRef::Primitive("u32".to_string()))]);

	assert_eq!(wire_schema_hash(&api, 1, 1).unwrap(), wire_schema_hash(&api, 1, 1).unwrap(),);
}

#[test]
fn type_signature_terminates_on_a_recursive_type() {
	// `struct Node { next: Option<Node> }` must not recurse forever.
	let node = TypeDef {
		name: "Node".to_string(),
		module_path: Vec::new(),
		generic_params: Vec::new(),
		kind: TypeDefKind::Struct(vec![FieldDef {
			name: "next".to_string(),
			type_ref: TypeRef::Option(Box::new(TypeRef::Named {
				name: "Node".to_string(),
				args: Vec::new(),
			})),
			docs: None,
		}]),
		docs: None,
	};
	let types: HashMap<&str, &TypeDef> = [("Node", &node)].into_iter().collect();

	let sig = type_signature(
		&TypeRef::Named { name: "Node".to_string(), args: Vec::new() },
		&types,
		&mut Vec::new(),
	);

	assert!(sig.contains("rec:Node"), "unexpected signature: {sig}");
}

#[test]
fn schema_hash_moves_when_a_compact_width_changes() {
	// `Compact<u32>` and `Compact<u64>` encode the same small values the same
	// way, so the frame length does not change - but the wider type accepts
	// values the narrower decoder rejects. The extractor used to discard the
	// argument entirely, collapsing every compact site to one token, so a
	// widening left the fingerprint byte-identical.
	let build = |width: &str| {
		api_with_payload_fields(vec![("size", TypeRef::Primitive(format!("compact<{width}>")))])
	};

	assert_ne!(
		wire_schema_hash(&build("u32"), 1, 1).unwrap(),
		wire_schema_hash(&build("u64"), 1, 1).unwrap(),
	);
}

#[test]
fn schema_hash_moves_when_an_enum_variant_is_reordered() {
	// Variant position is the SCALE discriminant, so a reorder silently
	// renumbers every variant on the wire.
	let variant = |name: &str| VariantDef {
		name: name.to_string(),
		fields: VariantFields::Unit,
		docs: None,
		codec_index: None,
	};
	let build = |names: [&str; 2]| {
		let enum_def = TypeDef {
			name: "Choice".to_string(),
			module_path: Vec::new(),
			generic_params: Vec::new(),
			kind: TypeDefKind::Enum(names.iter().map(|n| variant(n)).collect()),
			docs: None,
		};
		let method = MethodDef {
			name: "do_thing".to_string(),
			kind: MethodKind::Request,
			params: vec![ParamDef {
				name: "choice".to_string(),
				type_ref: TypeRef::Named { name: "Choice".to_string(), args: Vec::new() },
			}],
			return_type: ReturnType::Result { ok: TypeRef::Unit, err: TypeRef::Unit },
			wire: wire_attrs(Some(7)),
			docs: None,
		};
		ApiDefinition {
			traits: vec![TraitDef {
				name: "Thing".to_string(),
				module_path: Vec::new(),
				wire_trait_id: Some(200),
				methods: vec![method],
				docs: None,
			}],
			public_trait_order: vec!["Thing".to_string()],
			types: vec![enum_def],
			framework_types: Vec::new(),
		}
	};

	assert_ne!(
		wire_schema_hash(&build(["Allow", "Deny"]), 1, 1).unwrap(),
		wire_schema_hash(&build(["Deny", "Allow"]), 1, 1).unwrap(),
	);
}

#[test]
fn schema_hash_moves_when_a_method_changes_trait() {
	// The debugger derives its displayed method label from the generated const
	// name, which is trait-qualified (`LOCAL_STORAGE_WRITE` -> the label
	// `localStorage.write`). Hashing the bare method name therefore let a trait
	// rename, or a method moving between traits, keep the same fingerprint: the
	// host stamped a hash the debugger affirmatively confirmed, decode ran, and
	// every surface showed a confident wrong label - the exact failure the
	// fingerprint exists to refuse.
	let build = |trait_name: &str, trait_id: u8| {
		let method = MethodDef {
			name: "write".to_string(),
			kind: MethodKind::Request,
			params: Vec::new(),
			return_type: ReturnType::Result { ok: TypeRef::Unit, err: TypeRef::Unit },
			wire: wire_attrs(Some(7)),
			docs: None,
		};
		ApiDefinition {
			traits: vec![TraitDef {
				name: trait_name.to_string(),
				module_path: Vec::new(),
				wire_trait_id: Some(trait_id),
				methods: vec![method],
				docs: None,
			}],
			public_trait_order: vec![trait_name.to_string()],
			types: Vec::new(),
			framework_types: Vec::new(),
		}
	};

	// Same method id, same method name, same payload shape; only the trait
	// (name and id both) differs.
	assert_ne!(
		wire_schema_hash(&build("LocalStorage", 200), 1, 1).unwrap(),
		wire_schema_hash(&build("Statement", 201), 1, 1).unwrap(),
	);
}

#[test]
fn an_unresolvable_wire_reachable_type_fails_the_build() {
	// The guard that replaced an env-gated test which asserted nothing when
	// the variable was unset. `Missing` is referenced by the payload but is
	// absent from both `types` and `framework_types`, so its shape cannot be
	// fingerprinted - exactly the state `CallError` was in.
	let method = MethodDef {
		name: "do_thing".to_string(),
		kind: MethodKind::Request,
		params: vec![ParamDef {
			name: "request".to_string(),
			type_ref: TypeRef::Named { name: "Missing".to_string(), args: Vec::new() },
		}],
		return_type: ReturnType::Result { ok: TypeRef::Unit, err: TypeRef::Unit },
		wire: wire_attrs(Some(7)),
		docs: None,
	};
	let api = ApiDefinition {
		traits: vec![TraitDef {
			name: "Thing".to_string(),
			module_path: Vec::new(),
			wire_trait_id: Some(200),
			methods: vec![method],
			docs: None,
		}],
		public_trait_order: vec!["Thing".to_string()],
		types: Vec::new(),
		framework_types: Vec::new(),
	};

	let err =
		wire_schema_hash(&api, 1, 1).expect_err("an unresolvable payload type must fail codegen");
	assert!(format!("{err}").contains("Missing"), "the error must name the offending type: {err}");
}

#[test]
fn a_resolvable_payload_hashes_without_complaint() {
	// The negative control: the guard must not fire on an ordinary payload,
	// or every codegen run would fail.
	let api = api_with_payload_fields(vec![("ring_index", TypeRef::Primitive("u32".to_string()))]);

	assert!(wire_schema_hash(&api, 1, 1).is_ok());
}

#[test]
fn service_display_name_formats_known_acronyms() {
	let json_rpc = TraitDef {
		name: "JsonRpc".to_string(),
		module_path: Vec::new(),
		wire_trait_id: Some(198),
		methods: Vec::new(),
		docs: None,
	};
	let system = TraitDef {
		name: "System".to_string(),
		module_path: Vec::new(),
		wire_trait_id: Some(199),
		methods: Vec::new(),
		docs: None,
	};

	assert_eq!(service_display_name(&json_rpc), "JSON-RPC");
	assert_eq!(service_display_name(&system), "System");
}

fn request_method(name: &str, wire_id: Option<u8>) -> MethodDef {
	MethodDef {
		name: name.to_string(),
		kind: MethodKind::Request,
		params: Vec::new(),
		return_type: ReturnType::Result { ok: TypeRef::Unit, err: TypeRef::Unit },
		wire: wire_attrs(wire_id),
		docs: None,
	}
}

fn subscription_method(name: &str, wire_id: Option<u8>) -> MethodDef {
	MethodDef {
		name: name.to_string(),
		kind: MethodKind::Subscription,
		params: Vec::new(),
		return_type: ReturnType::Subscription {
			item: TypeRef::Unit,
			interrupt: TypeRef::Named {
				name: "CallError".to_string(),
				args: vec![TypeRef::Named { name: "GenericError".to_string(), args: vec![] }],
			},
		},
		wire: wire_attrs(wire_id),
		docs: None,
	}
}

fn api(methods: Vec<MethodDef>) -> ApiDefinition {
	ApiDefinition {
		traits: vec![TraitDef {
			name: "Example".to_string(),
			module_path: Vec::new(),
			wire_trait_id: Some(200),
			methods,
			docs: None,
		}],
		public_trait_order: Vec::new(),
		types: Vec::new(),
		framework_types: Vec::new(),
	}
}

fn named_type(name: &str) -> TypeRef {
	TypeRef::Named { name: name.to_string(), args: Vec::new() }
}

fn request_method_with_wrappers(
	name: &str,
	wire_id: Option<u8>,
	request: &str,
	response: &str,
	error: &str,
) -> MethodDef {
	MethodDef {
		name: name.to_string(),
		kind: MethodKind::Request,
		params: vec![ParamDef { name: "request".to_string(), type_ref: named_type(request) }],
		return_type: ReturnType::Result { ok: named_type(response), err: named_type(error) },
		wire: wire_attrs(wire_id),
		docs: None,
	}
}

fn subscription_method_with_wrappers(name: &str, wire_id: Option<u8>, item: &str) -> MethodDef {
	MethodDef {
		name: name.to_string(),
		kind: MethodKind::Subscription,
		params: Vec::new(),
		return_type: ReturnType::Subscription {
			item: named_type(item),
			interrupt: TypeRef::Named {
				name: "CallError".to_string(),
				args: vec![TypeRef::Named { name: "GenericError".to_string(), args: vec![] }],
			},
		},
		wire: wire_attrs(wire_id),
		docs: None,
	}
}

/// An empty struct `TypeDef`, so a synthetic fixture's payload types resolve.
/// A fixture that references a name it never defines is not a realistic API,
/// and the schema-hash guard rejects it for the same reason it rejects real
/// drift: an unresolvable type contributes only its name to the fingerprint.
fn empty_struct(name: &str) -> TypeDef {
	TypeDef {
		name: name.to_string(),
		module_path: Vec::new(),
		generic_params: Vec::new(),
		kind: TypeDefKind::Struct(Vec::new()),
		docs: None,
	}
}

fn versioned_tuple_wrapper_variants(name: &str, variants: &[(u32, &str)]) -> TypeDef {
	TypeDef {
		name: name.to_string(),
		module_path: vec!["truapi".into(), "versioned".into()],
		generic_params: Vec::new(),
		kind: TypeDefKind::Enum(
			variants
				.iter()
				.map(|(version, inner)| VariantDef {
					name: format!("V{version}"),
					fields: VariantFields::Unnamed(vec![named_type(inner)]),
					docs: None,
					codec_index: None,
				})
				.collect(),
		),
		docs: None,
	}
}

fn versioned_tuple_wrapper(name: &str, legacy: &str, latest: &str) -> TypeDef {
	versioned_tuple_wrapper_variants(name, &[(1, legacy), (2, latest)])
}

/// The framework types every subscription's interrupt leg reaches, in the
/// shape `truapi` declares them. Fixtures that emit a client need them, or
/// the wire schema hash refuses to fingerprint a bare name.
fn call_error_framework_types() -> Vec<TypeDef> {
	vec![
		TypeDef {
			name: "CallError".to_string(),
			module_path: Vec::new(),
			generic_params: vec!["D".to_string()],
			kind: TypeDefKind::Enum(vec![
				VariantDef {
					name: "Domain".to_string(),
					fields: VariantFields::Unnamed(vec![TypeRef::Generic("D".to_string())]),
					docs: None,
					codec_index: None,
				},
				VariantDef {
					name: "Denied".to_string(),
					fields: VariantFields::Unit,
					docs: None,
					codec_index: None,
				},
				VariantDef {
					name: "Unsupported".to_string(),
					fields: VariantFields::Unit,
					docs: None,
					codec_index: None,
				},
				VariantDef {
					name: "MalformedFrame".to_string(),
					fields: VariantFields::Named(vec![FieldDef {
						name: "reason".to_string(),
						type_ref: TypeRef::Primitive("String".to_string()),
						docs: None,
					}]),
					docs: None,
					codec_index: None,
				},
				VariantDef {
					name: "HostFailure".to_string(),
					fields: VariantFields::Named(vec![FieldDef {
						name: "reason".to_string(),
						type_ref: TypeRef::Primitive("String".to_string()),
						docs: None,
					}]),
					docs: None,
					codec_index: None,
				},
			]),
			docs: None,
		},
		single_field_struct("GenericError", "reason", "String"),
	]
}

fn single_field_struct(name: &str, field_name: &str, field_type: &str) -> TypeDef {
	TypeDef {
		name: name.to_string(),
		module_path: Vec::new(),
		generic_params: Vec::new(),
		kind: TypeDefKind::Struct(vec![FieldDef {
			name: field_name.to_string(),
			type_ref: TypeRef::Primitive(field_type.to_string()),
			docs: None,
		}]),
		docs: None,
	}
}

fn named_field_versioned_wrapper(name: &str) -> TypeDef {
	let fields = vec![
		FieldDef {
			name: "product_account_id".to_string(),
			type_ref: TypeRef::Named { name: "ProductAccountId".to_string(), args: Vec::new() },
			docs: None,
		},
		FieldDef {
			name: "context".to_string(),
			type_ref: TypeRef::Named { name: "Bytes".to_string(), args: Vec::new() },
			docs: None,
		},
	];
	TypeDef {
		name: name.to_string(),
		module_path: vec!["truapi".into(), "versioned".into()],
		generic_params: Vec::new(),
		kind: TypeDefKind::Enum(vec![
			VariantDef {
				name: "V1".to_string(),
				fields: VariantFields::Named(fields.clone()),
				docs: None,
				codec_index: None,
			},
			VariantDef {
				name: "V2".to_string(),
				fields: VariantFields::Named(fields),
				docs: None,
				codec_index: None,
			},
		]),
		docs: None,
	}
}

/// Several wrappers can select different versions of one base type, the
/// shape of `HostLocalStorage{Clear,Write}Error` sitting on V1 while
/// `HostLocalStorageReadError` has reached V2. The newest selected version
/// owns the unprefixed name whatever position it takes in the walk, so the
/// rule is asserted with that wrapper sorting first and sorting last:
/// selecting by last write or by first write passes one and fails the other.
#[test]
fn public_alias_for_a_shared_base_follows_the_newest_selected_version() {
	fn aliases_for(wrappers: &[(&str, u32, &str)]) -> BTreeMap<String, String> {
		let versioned = wrappers
			.iter()
			.map(|(name, version, inner)| {
				let ty = versioned_tuple_wrapper_variants(name, &[(*version, inner)]);
				let wrapper = detect_versioned_wrapper(&ty).expect("versioned wrapper");
				((*name).to_string(), wrapper)
			})
			.collect::<BTreeMap<_, _>>();
		let emit_versions = wrappers
			.iter()
			.map(|(name, version, _)| ((*name).to_string(), BTreeSet::from([*version])))
			.collect::<BTreeMap<_, _>>();

		selected_public_aliases(&api(Vec::new()), &versioned, &emit_versions, 2)
	}

	// The wrapper on the newest version sorts first, then last.
	for wrappers in [
		[
			("AReadError", 2, "V02Thing"),
			("BClearError", 1, "V01Thing"),
			("CWriteError", 1, "V01Thing"),
		],
		[
			("AClearError", 1, "V01Thing"),
			("BWriteError", 1, "V01Thing"),
			("CReadError", 2, "V02Thing"),
		],
	] {
		let aliases = aliases_for(&wrappers);

		assert_eq!(
			aliases.get("V02Thing").map(String::as_str),
			Some("Thing"),
			"the newest selected version owns the unprefixed name for {wrappers:?}: {aliases:?}"
		);
		assert!(
			!aliases.contains_key("V01Thing"),
			"the older version keeps its prefix for {wrappers:?}: {aliases:?}"
		);
	}
}

#[test]
fn detect_versioned_wrapper_keeps_each_versioned_variant() {
	let ty = TypeDef {
		name: "ExampleRequest".to_string(),
		module_path: vec!["truapi".into(), "versioned".into()],
		generic_params: Vec::new(),
		kind: TypeDefKind::Enum(vec![
			VariantDef {
				name: "V1".to_string(),
				fields: VariantFields::Unnamed(vec![TypeRef::Named {
					name: "LegacyRequest".to_string(),
					args: Vec::new(),
				}]),
				docs: None,
				codec_index: None,
			},
			VariantDef {
				name: "V10".to_string(),
				fields: VariantFields::Unnamed(vec![TypeRef::Named {
					name: "LatestRequest".to_string(),
					args: Vec::new(),
				}]),
				docs: None,
				codec_index: None,
			},
			VariantDef {
				name: "V2".to_string(),
				fields: VariantFields::Unnamed(vec![TypeRef::Named {
					name: "IntermediateRequest".to_string(),
					args: Vec::new(),
				}]),
				docs: None,
				codec_index: None,
			},
		]),
		docs: None,
	};

	let wrapper = detect_versioned_wrapper(&ty).expect("versioned wrapper");
	let legacy = wrapper.variants.get(&1).expect("V1 variant");
	let fallback = wrapper
		.variants
		.range(..=9)
		.next_back()
		.map(|(_, variant)| variant)
		.expect("V2 fallback");
	let latest = wrapper.variants.get(&10).expect("V10 variant");

	match &legacy.kind {
		VersionedKind::Tuple(TypeRef::Named { name, .. }) => {
			assert_eq!(name, "LegacyRequest");
		},
		other => panic!("unexpected wrapper kind: {other:?}"),
	}

	match &latest.kind {
		VersionedKind::Tuple(TypeRef::Named { name, .. }) => {
			assert_eq!(name, "LatestRequest");
		},
		other => panic!("unexpected wrapper kind: {other:?}"),
	}

	match &fallback.kind {
		VersionedKind::Tuple(TypeRef::Named { name, .. }) => {
			assert_eq!(name, "IntermediateRequest");
		},
		other => panic!("unexpected wrapper kind: {other:?}"),
	}
}

#[test]
fn latest_wire_version_falls_back_to_one_without_wrappers() {
	let api = ApiDefinition {
		traits: Vec::new(),
		public_trait_order: Vec::new(),
		types: Vec::new(),
		framework_types: Vec::new(),
	};
	assert_eq!(latest_wire_version(&api), 1);
}

#[test]
fn latest_wire_version_picks_highest_variant_across_wrappers() {
	let api = ApiDefinition {
		traits: Vec::new(),
		public_trait_order: Vec::new(),
		types: vec![
			versioned_tuple_wrapper_variants("OneWrapper", &[(1, "Legacy")]),
			versioned_tuple_wrapper_variants("TwoWrapper", &[(1, "Legacy"), (3, "Latest")]),
			versioned_tuple_wrapper_variants("ThreeWrapper", &[(2, "Middle")]),
		],
		framework_types: Vec::new(),
	};
	assert_eq!(latest_wire_version(&api), 3);
}

#[test]
fn generate_wire_table_emits_sorted_typescript_entries() {
	let source = generate_wire_table(
		&api(vec![request_method("later", Some(10)), subscription_method("stream", Some(2))]),
		2,
	)
	.expect("generate wire table");

	assert!(source.contains("export const EXAMPLE_STREAM = {"));
	assert!(source.contains("  trait: 200,"));
	assert!(source.contains("  method: 2,"));
	assert!(source.contains("export const EXAMPLE_LATER = {"));
	assert!(source.contains("  method: 10,"));
	assert!(
		source.find("export const EXAMPLE_STREAM").expect("stream entry") <
			source.find("export const EXAMPLE_LATER").expect("later entry")
	);
}

#[test]
fn generate_decode_table_emits_frame_keyed_decoders() {
	let api = ApiDefinition {
		traits: vec![TraitDef {
			name: "Example".to_string(),
			module_path: Vec::new(),
			wire_trait_id: Some(200),
			methods: vec![
				request_method_with_wrappers(
					"feature_supported",
					Some(2),
					"FeatureSupportedRequest",
					"FeatureSupportedResponse",
					"FeatureSupportedError",
				),
				subscription_method_with_wrappers("stream", Some(10), "StreamItem"),
			],
			docs: None,
		}],
		public_trait_order: vec!["Example".to_string()],
		types: vec![
			versioned_tuple_wrapper_variants(
				"FeatureSupportedRequest",
				&[(1, "FeatureSupportedRequestV1")],
			),
			versioned_tuple_wrapper_variants(
				"FeatureSupportedResponse",
				&[(1, "FeatureSupportedResponseV1")],
			),
			versioned_tuple_wrapper_variants(
				"FeatureSupportedError",
				&[(1, "FeatureSupportedErrorV1")],
			),
			versioned_tuple_wrapper_variants("StreamItem", &[(1, "StreamItemV1")]),
			empty_struct("FeatureSupportedRequestV1"),
			empty_struct("FeatureSupportedResponseV1"),
			empty_struct("FeatureSupportedErrorV1"),
			empty_struct("StreamItemV1"),
		],
		framework_types: Vec::new(),
	};

	let source = generate_decode_table(&api, 1).expect("generate decode table");

	assert!(source.contains("export const WIRE_DECODE_TABLE"));
	assert!(source.contains("(payload: Uint8Array) => unknown"));
	assert!(source.contains(
		"[W.EXAMPLE_FEATURE_SUPPORTED.trait * 256 + W.EXAMPLE_FEATURE_SUPPORTED.method]"
	));
	assert!(source.contains("[W.EXAMPLE_STREAM.trait * 256 + W.EXAMPLE_STREAM.method]"));
	// message_type 0 (Request/Start) and 1 (Response/Receive) both decode
	// straight through each leg's own versioned wrapper — no envelope.
	assert!(source.contains("T.VersionedFeatureSupportedRequest.dec(payload)"));
	assert!(source.contains(
		"S.Result(T.VersionedFeatureSupportedResponse, T.VersionedFeatureSupportedError).dec(payload)"
	));
	assert!(source.contains("T.VersionedStreamItem.dec(payload)"));
	// message_type 2 (Interrupt) on a subscription with no domain error
	// still decodes a framework-level `CallError<GenericError>`, in the
	// `Err` arm of the leg's `Result`.
	assert!(source.contains("S.Result(S._void, S.CallError(T.GenericError)).dec(payload)"));
}

#[test]
fn generate_wire_table_rejects_duplicate_ids() {
	let err = generate_wire_table(
		&api(vec![request_method("first", Some(2)), subscription_method("second", Some(2))]),
		2,
	)
	.expect_err("duplicate ids must error");

	assert!(err.to_string().contains("wire id (200, 2) reused"));
}

/// Trait 255 is reserved for protocol errors, so no API trait may declare
/// it. Kept byte-for-byte in step with the Rust emitter's
/// `wire_table_rejects_the_reserved_protocol_error_trait_id`: if the two
/// languages disagreed about which addresses are legal, one of them would
/// emit a table the other rejects.
#[test]
fn generate_wire_table_rejects_the_reserved_protocol_error_trait_id() {
	let mut api = api(vec![request_method("submit", Some(0))]);
	api.traits[0].wire_trait_id = Some(RESERVED_PROTOCOL_ERROR_TRAIT_ID);

	let error = generate_wire_table(&api, 2).expect_err("trait id 255 must be refused");
	let message = error.to_string();
	assert!(
		message.contains("wire trait id 255 reused") &&
			message.contains("reserved for protocol errors"),
		"unexpected error: {message}",
	);
}

/// The reservation must not have grown while moving up a level: under a
/// two-byte envelope `(8, 255)` is an ordinary address, and refusing it
/// would quietly cost every trait its last method slot.
#[test]
fn generate_wire_table_allows_method_id_255_outside_the_reserved_trait() {
	let method = request_method("explicit_request", Some(255));

	generate_wire_table(&api(vec![method]), 2).expect("(200, 255) is an ordinary address");
}

/// Version filtering must not become an escape hatch: a trait that declares
/// the reserved id is refused even when every one of its methods is excluded
/// from the target version. The trait id is validated before any method is
/// considered, which is what makes that hold.
#[test]
fn generate_wire_table_rejects_the_reserved_trait_id_for_filtered_methods() {
	let future = request_method_with_wrappers(
		"future",
		Some(0),
		"FutureRequest",
		"FutureResponse",
		"FutureError",
	);
	let api = ApiDefinition {
		traits: vec![TraitDef {
			name: "Example".to_string(),
			module_path: Vec::new(),
			wire_trait_id: Some(RESERVED_PROTOCOL_ERROR_TRAIT_ID),
			methods: vec![future],
			docs: None,
		}],
		public_trait_order: Vec::new(),
		framework_types: Vec::new(),
		types: vec![
			versioned_tuple_wrapper_variants("FutureRequest", &[(2, "FutureRequestV2")]),
			versioned_tuple_wrapper_variants("FutureResponse", &[(2, "FutureResponseV2")]),
			versioned_tuple_wrapper_variants("FutureError", &[(2, "FutureErrorV2")]),
		],
	};

	let error = generate_wire_table(&api, 1)
		.expect_err("a filtered-out method must not unlock the reserved trait id");
	assert!(
		error.to_string().contains("wire trait id 255 reused") &&
			error.to_string().contains("reserved for protocol errors"),
		"unexpected error: {error}",
	);
}

#[test]
fn generate_wire_table_rejects_missing_annotation() {
	let err = generate_wire_table(&api(vec![request_method("missing", None)]), 2)
		.expect_err("missing wire id must error");

	assert!(err.to_string().contains("missing #[wire(id = N)]"));
}

#[test]
fn generate_wire_table_emits_one_id_per_method_regardless_of_kind() {
	let request = request_method("custom_request", Some(2));
	let subscription = subscription_method("custom_stream", Some(20));

	let source = generate_wire_table(&api(vec![request, subscription]), 2).expect("wire table");

	assert!(source.contains("export const EXAMPLE_CUSTOM_REQUEST = {"));
	assert!(source.contains("  method: 2,"));
	assert!(source.contains("export const EXAMPLE_CUSTOM_STREAM = {"));
	assert!(source.contains("  method: 20,"));
}

/// Method ids are scoped per trait: two traits may both use method id 0.
#[test]
fn generate_wire_table_allows_same_method_id_in_different_traits() {
	let api = ApiDefinition {
		traits: vec![
			TraitDef {
				name: "Alpha".to_string(),
				module_path: Vec::new(),
				wire_trait_id: Some(193),
				methods: vec![request_method("first", Some(0))],
				docs: None,
			},
			TraitDef {
				name: "Beta".to_string(),
				module_path: Vec::new(),
				wire_trait_id: Some(194),
				methods: vec![request_method("second", Some(0))],
				docs: None,
			},
		],
		public_trait_order: Vec::new(),
		types: Vec::new(),
		framework_types: Vec::new(),
	};

	let source = generate_wire_table(&api, 2).expect("generate wire table");

	assert!(source.contains("export const ALPHA_FIRST = {"));
	assert!(source.contains("export const BETA_SECOND = {"));
	assert!(source.contains("  trait: 193,"));
	assert!(source.contains("  trait: 194,"));
}

/// Two traits must not share a wire trait id.
#[test]
fn generate_wire_table_rejects_duplicate_trait_ids() {
	let api = ApiDefinition {
		traits: vec![
			TraitDef {
				name: "Alpha".to_string(),
				module_path: Vec::new(),
				wire_trait_id: Some(195),
				methods: vec![request_method("first", Some(0))],
				docs: None,
			},
			TraitDef {
				name: "Beta".to_string(),
				module_path: Vec::new(),
				wire_trait_id: Some(195),
				methods: vec![request_method("second", Some(0))],
				docs: None,
			},
		],
		public_trait_order: Vec::new(),
		types: Vec::new(),
		framework_types: Vec::new(),
	};

	let err = generate_wire_table(&api, 2).expect_err("duplicate trait ids must error");
	assert!(err.to_string().contains("wire trait id 195 reused"));
}

/// A trait without `#[wire_trait(id = N)]` must fail emission.
#[test]
fn generate_wire_table_rejects_missing_trait_id() {
	let api = ApiDefinition {
		traits: vec![TraitDef {
			name: "Alpha".to_string(),
			module_path: Vec::new(),
			wire_trait_id: None,
			methods: vec![request_method("first", Some(0))],
			docs: None,
		}],
		public_trait_order: Vec::new(),
		types: Vec::new(),
		framework_types: Vec::new(),
	};

	let err = generate_wire_table(&api, 2).expect_err("missing trait id must error");
	assert!(err.to_string().contains("missing #[wire_trait(id = N)]"));
}

#[test]
fn generate_wire_table_filters_methods_by_target_version() {
	let api = ApiDefinition {
		traits: vec![TraitDef {
			name: "Example".to_string(),
			module_path: Vec::new(),
			wire_trait_id: Some(200),
			methods: vec![
				request_method_with_wrappers(
					"legacy",
					Some(2),
					"LegacyRequest",
					"LegacyResponse",
					"LegacyError",
				),
				request_method_with_wrappers(
					"future",
					Some(10),
					"FutureRequest",
					"FutureResponse",
					"FutureError",
				),
				subscription_method_with_wrappers("future_stream", Some(20), "FutureItem"),
			],
			docs: None,
		}],
		public_trait_order: Vec::new(),
		types: vec![
			versioned_tuple_wrapper_variants("LegacyRequest", &[(1, "LegacyRequestV1")]),
			versioned_tuple_wrapper_variants("LegacyResponse", &[(1, "LegacyResponseV1")]),
			versioned_tuple_wrapper_variants("LegacyError", &[(1, "LegacyErrorV1")]),
			versioned_tuple_wrapper_variants("FutureRequest", &[(2, "FutureRequestV2")]),
			versioned_tuple_wrapper_variants("FutureResponse", &[(2, "FutureResponseV2")]),
			versioned_tuple_wrapper_variants("FutureError", &[(2, "FutureErrorV2")]),
			versioned_tuple_wrapper_variants("FutureItem", &[(2, "FutureItemV2")]),
		],
		framework_types: Vec::new(),
	};

	let source = generate_wire_table(&api, 1).expect("generate wire table");

	assert!(source.contains("export const EXAMPLE_LEGACY = {"));
	assert!(source.contains("  method: 2,"));
	assert!(!source.contains("FUTURE"));
	assert!(!source.contains("FUTURE_STREAM"));
}

#[test]
fn generate_client_filters_empty_services_by_target_version() {
	let api = ApiDefinition {
		traits: vec![
			TraitDef {
				name: "Legacy".to_string(),
				module_path: Vec::new(),
				wire_trait_id: Some(201),
				methods: vec![request_method_with_wrappers(
					"legacy_call",
					Some(2),
					"LegacyRequest",
					"LegacyResponse",
					"LegacyError",
				)],
				docs: None,
			},
			TraitDef {
				name: "FutureOnly".to_string(),
				module_path: Vec::new(),
				wire_trait_id: Some(202),
				methods: vec![request_method_with_wrappers(
					"future_call",
					Some(4),
					"FutureRequest",
					"FutureResponse",
					"FutureError",
				)],
				docs: None,
			},
		],
		public_trait_order: vec!["Legacy".to_string(), "FutureOnly".to_string()],
		types: vec![
			versioned_tuple_wrapper_variants("LegacyRequest", &[(1, "LegacyRequestV1")]),
			versioned_tuple_wrapper_variants("LegacyResponse", &[(1, "LegacyResponseV1")]),
			versioned_tuple_wrapper_variants("LegacyError", &[(1, "LegacyErrorV1")]),
			versioned_tuple_wrapper_variants("FutureRequest", &[(2, "FutureRequestV2")]),
			versioned_tuple_wrapper_variants("FutureResponse", &[(2, "FutureResponseV2")]),
			versioned_tuple_wrapper_variants("FutureError", &[(2, "FutureErrorV2")]),
			versioned_tuple_wrapper_variants("LegacyVersion", &[(1, "LegacyVersionV1")]),
			versioned_tuple_wrapper_variants("FutureVersion", &[(2, "FutureVersionV2")]),
			empty_struct("LegacyErrorV1"),
			empty_struct("LegacyRequestV1"),
			empty_struct("LegacyResponseV1"),
		],
		framework_types: Vec::new(),
	};

	let source = generate_client(&api, 1, 1).expect("generate client");

	assert!(source.contains("export const TRUAPI_VERSION = 1 as const;"));
	assert!(source.contains("export const TRUAPI_CODEC_VERSION = 1 as const;"));
	assert!(source.contains("export class LegacyClient"));
	assert!(source.contains("legacyCall("));
	assert!(!source.contains("FutureOnlyClient"));
	assert!(!source.contains("futureCall("));
}

#[test]
fn generate_client_selects_highest_shared_wrapper_variant() {
	let api = ApiDefinition {
		traits: vec![TraitDef {
			name: "Example".to_string(),
			module_path: Vec::new(),
			wire_trait_id: Some(200),
			methods: vec![MethodDef {
				name: "example_call".to_string(),
				kind: MethodKind::Request,
				params: vec![ParamDef {
					name: "request".to_string(),
					type_ref: TypeRef::Named {
						name: "ExampleRequest".to_string(),
						args: Vec::new(),
					},
				}],
				return_type: ReturnType::Result {
					ok: TypeRef::Named { name: "ExampleResponse".to_string(), args: Vec::new() },
					err: TypeRef::Named { name: "ExampleError".to_string(), args: Vec::new() },
				},
				wire: wire_attrs(Some(2)),
				docs: None,
			}],
			docs: None,
		}],
		public_trait_order: vec!["Example".to_string()],
		types: vec![
			versioned_tuple_wrapper("ExampleRequest", "LegacyRequest", "LatestRequest"),
			versioned_tuple_wrapper("ExampleResponse", "LegacyResponse", "LatestResponse"),
			versioned_tuple_wrapper("ExampleError", "LegacyError", "LatestError"),
			empty_struct("LatestError"),
			empty_struct("LatestRequest"),
			empty_struct("LegacyError"),
			empty_struct("LatestResponse"),
			empty_struct("LegacyRequest"),
			empty_struct("LegacyResponse"),
		],
		framework_types: Vec::new(),
	};

	let client_source = generate_client(&api, 2, 1).expect("generate client");

	// V2 is the highest variant supported by every wrapper at or below the
	// target version. The codegen prefers the newest shared variant so
	// callers see the latest request/response shape the host advertises.
	assert!(client_source.contains("request: T.LatestRequest"));
	assert!(
		client_source
			.contains("payload: T.VersionedExampleRequest.enc({ tag: \"V2\", value: request }),")
	);
	assert!(client_source.contains("ResultAsync<T.LatestResponse, T.LatestError>"));
}

#[test]
fn generate_types_preserves_legacy_prefixed_wrapper_variants() {
	let api = ApiDefinition {
		traits: vec![TraitDef {
			name: "Example".to_string(),
			module_path: Vec::new(),
			wire_trait_id: Some(200),
			methods: vec![
				MethodDef {
					name: "legacy_call".to_string(),
					kind: MethodKind::Request,
					params: vec![ParamDef {
						name: "request".to_string(),
						type_ref: named_type("LegacyRequest"),
					}],
					return_type: ReturnType::Result {
						ok: TypeRef::Unit,
						err: named_type("ExampleError"),
					},
					wire: wire_attrs(Some(2)),
					docs: None,
				},
				MethodDef {
					name: "latest_call".to_string(),
					kind: MethodKind::Request,
					params: vec![ParamDef {
						name: "request".to_string(),
						type_ref: named_type("LatestRequest"),
					}],
					return_type: ReturnType::Result {
						ok: TypeRef::Unit,
						err: named_type("ExampleError"),
					},
					wire: wire_attrs(Some(4)),
					docs: None,
				},
			],
			docs: None,
		}],
		public_trait_order: vec!["Example".to_string()],
		types: vec![
			versioned_tuple_wrapper_variants("LegacyRequest", &[(1, "V01LegacyRequest")]),
			versioned_tuple_wrapper_variants("LatestRequest", &[(2, "V02LatestRequest")]),
			versioned_tuple_wrapper_variants(
				"ExampleError",
				&[(1, "V01ExampleError"), (2, "V02ExampleError")],
			),
			single_field_struct("V01LegacyRequest", "legacy_marker", "u8"),
			single_field_struct("V02LatestRequest", "latest_marker", "u32"),
			single_field_struct("V01ExampleError", "legacy_code", "u8"),
			single_field_struct("V02ExampleError", "latest_code", "u32"),
		],
		framework_types: Vec::new(),
	};

	let source = generate_types(&api, 2).expect("generate types");

	assert!(source.contains("export interface V01ExampleError"));
	assert!(source.contains("legacyCode: number;"));
	assert!(source.contains("export interface ExampleError"));
	assert!(source.contains("latestCode: number;"));
	assert!(!source.contains("export interface V02ExampleError"));
	assert!(source.contains(r#"{ tag: "V1"; value: V01ExampleError }"#));
	assert!(source.contains(r#"{ tag: "V2"; value: ExampleError }"#));
	assert!(source.contains(
		"S.indexedTaggedUnion({V1: [0, V01ExampleError] as const, V2: [1, ExampleError] as const})"
	));
}

#[test]
fn generate_client_uses_only_existing_wrapper_variant() {
	let api = ApiDefinition {
		traits: vec![TraitDef {
			name: "Example".to_string(),
			module_path: Vec::new(),
			wire_trait_id: Some(200),
			methods: vec![MethodDef {
				name: "example_call".to_string(),
				kind: MethodKind::Request,
				params: vec![ParamDef {
					name: "request".to_string(),
					type_ref: TypeRef::Named {
						name: "ExampleRequest".to_string(),
						args: Vec::new(),
					},
				}],
				return_type: ReturnType::Result {
					ok: TypeRef::Named { name: "ExampleResponse".to_string(), args: Vec::new() },
					err: TypeRef::Named { name: "ExampleError".to_string(), args: Vec::new() },
				},
				wire: wire_attrs(Some(2)),
				docs: None,
			}],
			docs: None,
		}],
		public_trait_order: vec!["Example".to_string()],
		types: vec![
			versioned_tuple_wrapper_variants("ExampleRequest", &[(1, "LegacyRequest")]),
			versioned_tuple_wrapper("ExampleResponse", "LegacyResponse", "LatestResponse"),
			versioned_tuple_wrapper("ExampleError", "LegacyError", "LatestError"),
			empty_struct("LatestError"),
			empty_struct("LatestResponse"),
			empty_struct("LegacyError"),
			empty_struct("LegacyRequest"),
			empty_struct("LegacyResponse"),
		],
		framework_types: Vec::new(),
	};

	let client_source = generate_client(&api, 2, 1).expect("generate client");

	assert!(client_source.contains("request: T.LegacyRequest"));
	assert!(
		client_source
			.contains("payload: T.VersionedExampleRequest.enc({ tag: \"V1\", value: request }),")
	);
	assert!(client_source.contains("ResultAsync<T.LegacyResponse, T.LegacyError>"));
}

/// A subscription declaring a `Start` parameter that is not a versioned
/// wrapper has no wire payload for it. The subscription arms would emit an
/// empty `Start` while `emit_payload` had already put the parameter in the
/// signature, so the generated call would silently drop its argument. The
/// Rust dispatcher emitter rejects the same input, and `main.rs` writes the
/// TS client before that runs, so this has to fail here too.
#[test]
fn generate_client_rejects_a_non_wrapper_subscription_start_param() {
	let api = ApiDefinition {
		traits: vec![TraitDef {
			name: "Example".to_string(),
			module_path: Vec::new(),
			wire_trait_id: Some(200),
			methods: vec![MethodDef {
				name: "example_subscribe".to_string(),
				kind: MethodKind::Subscription,
				params: vec![ParamDef {
					name: "request".to_string(),
					type_ref: TypeRef::Primitive("u32".to_string()),
				}],
				return_type: ReturnType::Subscription {
					item: TypeRef::Named { name: "ExampleItem".to_string(), args: Vec::new() },
					interrupt: TypeRef::Named {
						name: "CallError".to_string(),
						args: vec![TypeRef::Named {
							name: "GenericError".to_string(),
							args: Vec::new(),
						}],
					},
				},
				wire: wire_attrs(Some(2)),
				docs: None,
			}],
			docs: None,
		}],
		// The trait has to be public, or the service is filtered out before
		// `emit_method` runs and the guard under test is never reached.
		public_trait_order: vec!["Example".to_string()],
		types: vec![
			versioned_tuple_wrapper("ExampleItem", "LegacyItem", "LatestItem"),
			single_field_struct("LegacyItem", "value", "u32"),
			single_field_struct("LatestItem", "value", "u32"),
		],
		framework_types: call_error_framework_types(),
	};

	let err = generate_client(&api, 2, 1).expect_err("a raw Start param must be rejected");

	assert!(
		err.to_string()
			.contains("request parameter is not a recognized versioned wrapper"),
		"unexpected error: {err}"
	);
}

#[test]
fn generate_client_rejects_named_field_versioned_wrapper() {
	let api = ApiDefinition {
		traits: vec![TraitDef {
			name: "Example".to_string(),
			module_path: Vec::new(),
			wire_trait_id: Some(200),
			methods: vec![MethodDef {
				name: "example_call".to_string(),
				kind: MethodKind::Request,
				params: vec![ParamDef {
					name: "request".to_string(),
					type_ref: TypeRef::Named {
						name: "ExampleRequest".to_string(),
						args: Vec::new(),
					},
				}],
				return_type: ReturnType::Result {
					ok: TypeRef::Named { name: "ExampleResponse".to_string(), args: Vec::new() },
					err: TypeRef::Unit,
				},
				wire: wire_attrs(Some(2)),
				docs: None,
			}],
			docs: None,
		}],
		public_trait_order: Vec::new(),
		types: vec![
			named_field_versioned_wrapper("ExampleRequest"),
			versioned_tuple_wrapper("ExampleResponse", "LegacyResponse", "LatestResponse"),
		],
		framework_types: Vec::new(),
	};

	let err = generate_client(&api, 2, 1).expect_err("named field wrapper rejected");

	assert!(err.to_string().contains("uses named fields"));
}
