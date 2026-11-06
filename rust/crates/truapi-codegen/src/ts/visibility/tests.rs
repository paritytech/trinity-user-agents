use super::*;
use crate::rustdoc::{FieldDef, MethodKind, ParamDef, TraitDef, VariantDef, WireAttrs};

fn named(name: &str) -> TypeRef {
	TypeRef::Named { name: name.into(), args: Vec::new() }
}

fn type_def(name: &str, kind: TypeDefKind) -> TypeDef {
	TypeDef {
		name: name.into(),
		module_path: Vec::new(),
		generic_params: Vec::new(),
		kind,
		docs: None,
	}
}

fn field(type_ref: TypeRef) -> FieldDef {
	FieldDef { name: "value".into(), type_ref, docs: None }
}

fn method(internal: bool, request: &str, return_type: ReturnType) -> MethodDef {
	MethodDef {
		name: if internal { "authorize" } else { "request" }.into(),
		kind: if matches!(return_type, ReturnType::Subscription { .. }) {
			MethodKind::Subscription
		} else {
			MethodKind::Request
		},
		params: vec![ParamDef { name: "request".into(), type_ref: named(request) }],
		return_type,
		wire: WireAttrs { internal, ..WireAttrs::default() },
		docs: None,
	}
}

fn api(methods: Vec<MethodDef>, types: Vec<TypeDef>) -> ApiDefinition {
	ApiDefinition {
		traits: vec![TraitDef {
			name: "Permissions".into(),
			module_path: Vec::new(),
			wire_trait_id: Some(10),
			methods,
			docs: None,
		}],
		public_trait_order: vec!["Permissions".into()],
		types,
		framework_types: Vec::new(),
	}
}

#[test]
fn hides_internal_payloads_and_cycles_while_preserving_shared_types() {
	let api = api(
		vec![
			method(
				true,
				"InternalRequest",
				ReturnType::Result { ok: named("InternalResponse"), err: named("InternalError") },
			),
			method(
				false,
				"Shared",
				ReturnType::Subscription {
					item: named("PublicEvent"),
					interrupt: named("SharedInterrupt"),
				},
			),
		],
		vec![
			type_def(
				"InternalRequest",
				TypeDefKind::Enum(vec![VariantDef {
					name: "V1".into(),
					fields: VariantFields::Unnamed(vec![named("InternalPayload")]),
					docs: None,
					codec_index: None,
				}]),
			),
			type_def(
				"InternalPayload",
				TypeDefKind::Struct(vec![field(TypeRef::Option(Box::new(TypeRef::Vec(
					Box::new(TypeRef::Tuple(vec![named("CycleA"), named("Shared")])),
				))))]),
			),
			type_def("CycleA", TypeDefKind::Alias(named("CycleB"))),
			type_def(
				"CycleB",
				TypeDefKind::Struct(vec![field(TypeRef::Option(Box::new(named("CycleA"))))]),
			),
			type_def(
				"InternalResponse",
				TypeDefKind::TupleStruct(vec![TypeRef::Named {
					name: "Envelope".into(),
					args: vec![named("PrivateLeaf")],
				}]),
			),
			TypeDef {
				generic_params: vec!["T".into()],
				..type_def(
					"Envelope",
					TypeDefKind::Struct(vec![field(TypeRef::Generic("T".into()))]),
				)
			},
			type_def("PrivateLeaf", TypeDefKind::Alias(TypeRef::Unit)),
			type_def(
				"InternalError",
				TypeDefKind::Alias(TypeRef::Array(Box::new(named("SharedInterrupt")), 2)),
			),
			type_def("Shared", TypeDefKind::Alias(named("SharedNested"))),
			type_def("SharedNested", TypeDefKind::Alias(TypeRef::Primitive("String".into()))),
			type_def("SharedInterrupt", TypeDefKind::Alias(TypeRef::Unit)),
			type_def("PublicEvent", TypeDefKind::Alias(TypeRef::Unit)),
			type_def("UnrelatedHelper", TypeDefKind::Alias(TypeRef::Unit)),
		],
	);

	assert_eq!(
		internal_type_names(&api),
		[
			"CycleA",
			"CycleB",
			"Envelope",
			"InternalError",
			"InternalPayload",
			"InternalRequest",
			"InternalResponse",
			"PrivateLeaf",
		]
		.map(String::from)
		.into()
	);
}

#[test]
fn existing_public_helpers_keep_their_internal_method_dependencies_public() {
	let api = api(
		vec![method(
			true,
			"InternalRequest",
			ReturnType::Result { ok: TypeRef::Unit, err: TypeRef::Unit },
		)],
		vec![
			type_def("InternalRequest", TypeDefKind::Alias(named("Payload"))),
			type_def("Payload", TypeDefKind::Alias(named("Nested"))),
			type_def("Nested", TypeDefKind::Alias(TypeRef::Unit)),
			type_def("PublicHelper", TypeDefKind::Alias(named("Payload"))),
		],
	);

	assert_eq!(internal_type_names(&api), BTreeSet::from(["InternalRequest".into()]));
}

#[test]
fn direct_dependencies_include_generic_arguments_and_named_variant_fields() {
	let ty = type_def(
		"Response",
		TypeDefKind::Enum(vec![
			VariantDef {
				name: "Empty".into(),
				fields: VariantFields::Unit,
				docs: None,
				codec_index: None,
			},
			VariantDef {
				name: "Value".into(),
				fields: VariantFields::Named(vec![field(TypeRef::Named {
					name: "Envelope".into(),
					args: vec![TypeRef::Array(
						Box::new(TypeRef::Option(Box::new(named("Shared")))),
						2,
					)],
				})]),
				docs: None,
				codec_index: None,
			},
		]),
	);

	assert_eq!(type_dependencies(&ty), ["Envelope", "Shared"].map(String::from).into());
}

#[test]
fn hex_string_import_is_required_only_for_byte_collections() {
	let types = [
		TypeRef::Vec(Box::new(TypeRef::Primitive("u8".into()))),
		TypeRef::Array(Box::new(TypeRef::Primitive("u8".into())), 32),
		TypeRef::Vec(Box::new(TypeRef::Primitive("u32".into()))),
		TypeRef::Primitive("u8".into()),
	];
	let actual = types.map(|ty| {
		uses_hex_string(&type_def(
			"Response",
			TypeDefKind::Struct(vec![field(TypeRef::Option(Box::new(TypeRef::Named {
				name: "Envelope".into(),
				args: vec![TypeRef::Tuple(vec![ty])],
			})))]),
		))
	});

	assert_eq!(actual, [true, true, false, false]);
}
