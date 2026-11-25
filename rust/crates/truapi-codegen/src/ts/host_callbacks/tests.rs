use super::*;

fn named(name: &str) -> TypeRef {
	TypeRef::Named { name: name.to_string(), args: Vec::new() }
}

fn codec_types() -> BTreeSet<String> {
	["GenericError", "HostFeatureSupportedRequest", "HostFeatureSupportedResponse"]
		.into_iter()
		.map(str::to_string)
		.collect()
}

#[test]
fn local_codec_imports_external_alias_as_runtime_value() {
	let definition = PlatformDefinition {
		traits: Vec::new(),
		types: vec![TypeDef {
			name: "SessionUiInfo".to_string(),
			module_path: vec!["truapi".to_string(), "platform".to_string()],
			generic_params: Vec::new(),
			kind: TypeDefKind::Struct(vec![FieldDef {
				name: "public_key".to_string(),
				type_ref: named("Bytes32"),
				docs: None,
			}]),
			docs: None,
		}],
		super_trait: None,
		optional_super_trait: None,
	};
	let local_codec_types = ["SessionUiInfo".to_string()].into_iter().collect();

	let output =
		emit_host_callbacks(&definition, &local_codec_types).expect("host callbacks should render");

	assert!(
		output.contains("import {\n  Bytes32,\n} from \"@parity/truapi\";"),
		"external aliases used by a local codec must be runtime imports: {output}"
	);
	assert!(!output.contains("import type {\n  Bytes32,"));
	assert!(output.contains("S.Struct({publicKey: Bytes32})"));
}

fn platform_with_method(method: PlatformMethod) -> PlatformDefinition {
	PlatformDefinition {
		traits: vec![PlatformTrait {
			name: "Features".to_string(),
			docs: None,
			methods: vec![method],
		}],
		types: Vec::new(),
		super_trait: None,
		optional_super_trait: None,
	}
}

fn method_with_return(ok: TypeRef) -> PlatformMethod {
	PlatformMethod {
		name: "feature_supported".to_string(),
		docs: None,
		params: vec![PlatformParam {
			name: "request".to_string(),
			type_ref: named("HostFeatureSupportedRequest"),
			borrowed: false,
		}],
		return_shape: PlatformReturn {
			is_async: true,
			inner: PlatformInner::Result { ok, err: named("GenericError") },
		},
		has_default: false,
	}
}

fn method_with_param(param: TypeRef) -> PlatformMethod {
	PlatformMethod {
		name: "feature_supported".to_string(),
		docs: None,
		params: vec![PlatformParam {
			name: "request".to_string(),
			type_ref: param,
			borrowed: false,
		}],
		return_shape: PlatformReturn {
			is_async: true,
			inner: PlatformInner::Result {
				ok: named("HostFeatureSupportedResponse"),
				err: named("GenericError"),
			},
		},
		has_default: false,
	}
}

fn assert_rejects_compound_codec(method: PlatformMethod, expected: &str) {
	let definition = platform_with_method(method);
	let err = emit_wasm_adapter(&definition, &codec_types(), &BTreeSet::new())
		.expect_err("compound codec boundary should fail codegen")
		.to_string();
	assert!(err.contains("unsupported compound codec type"), "unexpected error: {err}");
	assert!(err.contains(expected), "unexpected error: {err}");
}

#[test]
fn wasm_adapter_rejects_compound_codec_return_shapes() {
	let codec = named("HostFeatureSupportedResponse");
	assert_rejects_compound_codec(
		method_with_return(TypeRef::Vec(Box::new(codec.clone()))),
		"return value",
	);
	assert_rejects_compound_codec(
		method_with_return(TypeRef::Option(Box::new(TypeRef::Vec(Box::new(codec.clone()))))),
		"return value",
	);
	assert_rejects_compound_codec(method_with_return(TypeRef::Tuple(vec![codec])), "return value");
}

#[test]
fn wasm_adapter_rejects_compound_codec_param_shapes() {
	let codec = named("HostFeatureSupportedRequest");
	assert_rejects_compound_codec(
		method_with_param(TypeRef::Vec(Box::new(codec.clone()))),
		"parameter `request`",
	);
	assert_rejects_compound_codec(
		method_with_param(TypeRef::Option(Box::new(codec.clone()))),
		"parameter `request`",
	);
	assert_rejects_compound_codec(
		method_with_param(TypeRef::Tuple(vec![codec])),
		"parameter `request`",
	);
}

#[test]
fn host_callbacks_encode_and_decode_the_latest_registration_status() {
	let status = "ChatRoomRegistrationStatus";
	let status_type = |version: u8, kind| TypeDef {
		name: format!("V{version:02}{status}"),
		module_path: vec!["truapi".into(), format!("v{version:02}"), "chat".into()],
		generic_params: Vec::new(),
		kind,
		docs: None,
	};
	let variants = ["New", "Exists"].map(|name| VariantDef {
		name: name.into(),
		fields: VariantFields::Unit,
		codec_index: None,
		docs: None,
	});
	let api = ApiDefinition {
		types: vec![
			status_type(2, TypeDefKind::Enum(variants.into())),
			status_type(1, TypeDefKind::Alias(TypeRef::Primitive("u8".into()))),
		],
		traits: Vec::new(),
		public_trait_order: Vec::new(),
		framework_types: Vec::new(),
	};
	let mut method = method_with_return(named(status));
	method.name = "create_chat_room".into();
	method.params.clear();
	let platform = platform_with_method(method);
	let directory = tempfile::tempdir().unwrap();
	let output = directory.path().to_str().unwrap();
	generate(&platform, &api, output, output).unwrap();
	let adapter = fs::read_to_string(directory.path().join("host-callbacks-adapter.ts")).unwrap();
	assert!(adapter.contains("ChatRoomRegistrationStatus.enc(await "), "{adapter}");
	let bridge = crate::rust::generate_wasm_bridge(&platform, &api).unwrap();
	assert!(bridge.contains("decode_bytes::<latest::ChatRoomRegistrationStatus>"), "{bridge}");
}

// A host answers an optional codec result with a value or `undefined`. The
// adapter encodes only a value and the bridge decodes only bytes, so
// `undefined` reaches the core as `None` and never as bytes that fail to
// decode.
#[test]
fn an_optional_codec_result_crosses_the_bridge_as_bytes_or_nothing() {
	let response = "HostFeatureSupportedResponse";
	let api = ApiDefinition {
		types: vec![TypeDef {
			name: response.into(),
			module_path: vec!["truapi".into(), "v01".into(), "features".into()],
			generic_params: Vec::new(),
			kind: TypeDefKind::Struct(vec![FieldDef {
				name: "supported".into(),
				type_ref: TypeRef::Primitive("bool".into()),
				docs: None,
			}]),
			docs: None,
		}],
		traits: Vec::new(),
		public_trait_order: Vec::new(),
		framework_types: Vec::new(),
	};
	let mut method = method_with_return(TypeRef::Option(Box::new(named(response))));
	method.params.clear();
	let platform = platform_with_method(method);

	let adapter = emit_wasm_adapter(&platform, &codec_types(), &BTreeSet::new())
		.expect("an optional codec result renders");
	assert!(
		adapter.contains(
			"const value = await callbacks.features.featureSupported(); return value === undefined ? undefined : HostFeatureSupportedResponse.enc(value);"
		),
		"{adapter}"
	);
	let bridge = crate::rust::generate_wasm_bridge(&platform, &api).unwrap();
	assert!(bridge.contains("invoke_optional_bytes_return("), "{bridge}");
	assert!(
		bridge.contains(".map(|bytes| decode_bytes::<latest::HostFeatureSupportedResponse>("),
		"{bridge}"
	);
}
