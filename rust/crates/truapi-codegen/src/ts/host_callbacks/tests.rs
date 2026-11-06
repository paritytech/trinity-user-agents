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
		method_with_return(TypeRef::Option(Box::new(codec.clone()))),
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
