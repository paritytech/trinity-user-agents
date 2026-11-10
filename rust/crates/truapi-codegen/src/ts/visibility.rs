use std::collections::{BTreeMap, BTreeSet};

use crate::rustdoc::{
	ApiDefinition, MethodDef, ReturnType, TypeDef, TypeDefKind, TypeRef, VariantFields,
};

pub fn internal_type_names(api: &ApiDefinition) -> BTreeSet<String> {
	let graph: BTreeMap<_, _> =
		api.types.iter().map(|ty| (ty.name.as_str(), type_dependencies(ty))).collect();
	let mut internal_roots = BTreeSet::new();
	let mut public_roots = BTreeSet::new();
	for method in api.traits.iter().flat_map(|trait_def| &trait_def.methods) {
		let roots = if method.wire.internal { &mut internal_roots } else { &mut public_roots };
		method_dependencies(method, roots);
	}
	let internal = reachable_types(&graph, internal_roots);
	public_roots.extend(
		api.types
			.iter()
			.filter(|ty| !internal.contains(&ty.name))
			.map(|ty| ty.name.clone()),
	);
	let public = reachable_types(&graph, public_roots);
	internal.difference(&public).cloned().collect()
}

fn reachable_types(
	graph: &BTreeMap<&str, BTreeSet<String>>,
	roots: BTreeSet<String>,
) -> BTreeSet<String> {
	let mut reachable = BTreeSet::new();
	let mut pending: Vec<_> = roots.into_iter().collect();
	while let Some(name) = pending.pop() {
		let Some(dependencies) = graph.get(name.as_str()) else {
			continue;
		};
		if reachable.insert(name) {
			pending.extend(dependencies.iter().cloned());
		}
	}
	reachable
}

fn method_dependencies(method: &MethodDef, names: &mut BTreeSet<String>) {
	for param in &method.params {
		collect_names(&param.type_ref, names);
	}
	match &method.return_type {
		ReturnType::Result { ok, err } => {
			collect_names(ok, names);
			collect_names(err, names);
		},
		ReturnType::Subscription { item, interrupt } => {
			collect_names(item, names);
			collect_names(interrupt, names);
		},
	}
}

pub fn type_dependencies(ty: &TypeDef) -> BTreeSet<String> {
	let mut names = BTreeSet::new();
	visit_fields(ty, &mut |field| collect_names(field, &mut names));
	names
}

pub fn type_ref_dependencies(ty: &TypeRef) -> BTreeSet<String> {
	let mut names = BTreeSet::new();
	collect_names(ty, &mut names);
	names
}

pub fn uses_hex_string(ty: &TypeDef) -> bool {
	let mut uses_hex_string = false;
	visit_fields(ty, &mut |field| {
		uses_hex_string |= type_ref_uses_hex_string(field);
	});
	uses_hex_string
}

pub fn type_ref_uses_hex_string(ty: &TypeRef) -> bool {
	let mut uses_hex_string = false;
	visit_type_ref(ty, &mut |ty| {
		if let TypeRef::Vec(inner) | TypeRef::Array(inner, _) = ty {
			uses_hex_string |= matches!(inner.as_ref(), TypeRef::Primitive(name) if name == "u8");
		}
	});
	uses_hex_string
}

fn visit_fields(ty: &TypeDef, visit: &mut impl FnMut(&TypeRef)) {
	match &ty.kind {
		TypeDefKind::Alias(inner) => visit(inner),
		TypeDefKind::Struct(fields) => {
			for field in fields {
				visit(&field.type_ref);
			}
		},
		TypeDefKind::TupleStruct(fields) => {
			for field in fields {
				visit(field);
			}
		},
		TypeDefKind::Enum(variants) => {
			for variant in variants {
				match &variant.fields {
					VariantFields::Unit => {},
					VariantFields::Unnamed(fields) => {
						for field in fields {
							visit(field);
						}
					},
					VariantFields::Named(fields) => {
						for field in fields {
							visit(&field.type_ref);
						}
					},
				}
			}
		},
	}
}

fn collect_names(ty: &TypeRef, names: &mut BTreeSet<String>) {
	visit_type_ref(ty, &mut |ty| {
		if let TypeRef::Named { name, .. } = ty {
			names.insert(name.clone());
		}
	});
}

fn visit_type_ref(ty: &TypeRef, visit: &mut impl FnMut(&TypeRef)) {
	visit(ty);
	match ty {
		TypeRef::Named { args, .. } | TypeRef::Tuple(args) => {
			for arg in args {
				visit_type_ref(arg, visit);
			}
		},
		TypeRef::Vec(inner) | TypeRef::Option(inner) | TypeRef::Array(inner, _) => {
			visit_type_ref(inner, visit);
		},
		TypeRef::Primitive(_) | TypeRef::Generic(_) | TypeRef::Unit => {},
	}
}

#[cfg(test)]
mod tests {
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
					ReturnType::Result {
						ok: named("InternalResponse"),
						err: named("InternalError"),
					},
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
}
