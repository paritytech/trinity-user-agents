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
mod tests;
