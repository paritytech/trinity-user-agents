//! Shared callback naming and selection rules for generated platform bridges.

use std::collections::BTreeSet;

use crate::platform::{PlatformDefinition, PlatformInner, PlatformMethod, PlatformTrait};
use crate::rustdoc::{TypeDef, TypeDefKind, TypeRef, VariantFields};

/// Compound callback results with inline SCALE codecs shared by both bridges.
/// Byte vectors themselves remain unencoded byte payloads.
pub(crate) fn is_scale_vector_result(ty: &TypeRef) -> bool {
    matches!(
        ty,
        TypeRef::Vec(inner)
            if matches!(inner.as_ref(), TypeRef::Array(element, _)
                if matches!(element.as_ref(), TypeRef::Primitive(name) if name == "u8"))
                || matches!(inner.as_ref(), TypeRef::Primitive(name) if name == "str")
                || matches!(inner.as_ref(), TypeRef::Named { args, .. } if args.is_empty())
    )
}

/// Traits the platform surface actually composes: the super trait's
/// constituents when one exists, otherwise every collected trait.
pub fn composed_traits(definition: &PlatformDefinition) -> Vec<&PlatformTrait> {
    let mut composed: BTreeSet<String> = match &definition.super_trait {
        Some(s) => s.composes.iter().cloned().collect(),
        None => definition.traits.iter().map(|t| t.name.clone()).collect(),
    };
    composed.extend(optional_trait_names(definition));
    definition
        .traits
        .iter()
        .filter(|t| composed.contains(&t.name))
        .collect()
}

/// Capability trait names a host may omit, taken from the `OptionalPlatform`
/// super-trait. A host that supplies none of a trait's callbacks is not
/// broken: the core applies the capability's absence behavior.
pub fn optional_trait_names(definition: &PlatformDefinition) -> BTreeSet<String> {
    definition
        .optional_super_trait
        .as_ref()
        .map(|s| s.composes.iter().cloned().collect())
        .unwrap_or_default()
}

/// JS-side callback name for a platform method (camelCase of the Rust name).
pub fn raw_callback_name(method: &PlatformMethod) -> String {
    to_camel_case(&method.name)
}

/// Set of all platform trait names, used to recognize trait-object returns.
pub fn platform_trait_names(definition: &PlatformDefinition) -> BTreeSet<String> {
    definition.traits.iter().map(|t| t.name.clone()).collect()
}

/// Name of the platform trait a method returns as a handle, if any.
pub fn trait_object_return_name<'a>(
    method: &'a PlatformMethod,
    platform_trait_names: &BTreeSet<String>,
) -> Option<&'a str> {
    match &method.return_shape.inner {
        PlatformInner::TraitObject(name) => Some(name.as_str()),
        PlatformInner::Result { ok, .. } | PlatformInner::Plain(ok) => {
            named_platform_trait(ok, platform_trait_names)
        }
        PlatformInner::Unit | PlatformInner::Stream(_) => None,
    }
}

/// Wire name of a raw callback. Handle-returning methods get a trait
/// namespace prefix so equally named methods on different traits stay
/// distinct.
pub fn raw_callback_wire_name(
    trait_def: &PlatformTrait,
    method: &PlatformMethod,
    platform_trait_names: &BTreeSet<String>,
) -> String {
    // The public Rust method spells out its capability; the established raw
    // connection bridge uses the same namespace-first shape as chainConnect.
    if trait_def.name == "HopProvider" && method.name == "connect_hop" {
        return "hopConnect".to_string();
    }
    let raw = raw_callback_name(method);
    if trait_object_return_name(method, platform_trait_names).is_some() {
        return format!(
            "{}{}",
            callback_namespace(&trait_def.name),
            upper_first(&raw)
        );
    }
    raw
}

/// Field name holding the callback in the generated Rust bridge struct.
pub fn raw_callback_field_name(
    trait_def: &PlatformTrait,
    method: &PlatformMethod,
    platform_trait_names: &BTreeSet<String>,
) -> String {
    snake_case(&raw_callback_wire_name(
        trait_def,
        method,
        platform_trait_names,
    ))
}

/// TS type name for the raw callback in the generated host-callback bridge.
pub fn raw_callback_type_name(
    trait_def: &PlatformTrait,
    method: &PlatformMethod,
    platform_trait_names: &BTreeSet<String>,
) -> String {
    upper_first(&raw_callback_wire_name(
        trait_def,
        method,
        platform_trait_names,
    ))
}

/// Name of the TS adapter that wraps a typed host callback into its raw form.
pub fn raw_callback_adapter_name(
    trait_def: &PlatformTrait,
    method: &PlatformMethod,
    platform_trait_names: &BTreeSet<String>,
) -> String {
    format!(
        "{}Adapter",
        raw_callback_wire_name(trait_def, method, platform_trait_names)
    )
}

/// Callback-object namespace for a trait: its name with the role suffix
/// (`Provider`, `Presenter`, `Host`) stripped, lower-cased first letter.
pub fn callback_namespace(trait_name: &str) -> String {
    let stem = ["Provider", "Presenter", "Host", "Platform"]
        .into_iter()
        .find_map(|suffix| trait_name.strip_suffix(suffix))
        .unwrap_or(trait_name);
    lower_pascal_case(stem)
}

fn named_platform_trait<'a>(
    ty: &'a TypeRef,
    platform_trait_names: &BTreeSet<String>,
) -> Option<&'a str> {
    let TypeRef::Named { name, args } = ty else {
        return None;
    };
    if args.is_empty() && platform_trait_names.contains(name) {
        return Some(name.as_str());
    }
    None
}

/// Unwrap a `Result<T, E>` stream item to its `T`; other item types pass
/// through. Streams carry `Result`s on the Rust side but the JS raw bridge
/// already unwraps them before handing each item to the WASM callback sink.
pub fn stream_item(item: &TypeRef) -> &TypeRef {
    if let TypeRef::Named { name, args } = item
        && name == "Result"
        && let Some(ok) = args.first()
    {
        return ok;
    }
    item
}

/// Convert a snake_case identifier to camelCase.
pub fn to_camel_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper_next = false;
    for (idx, ch) in name.chars().enumerate() {
        if ch == '_' {
            upper_next = idx != 0;
            continue;
        }
        if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn lower_pascal_case(name: &str) -> String {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!(
        "{}{}",
        first.to_ascii_lowercase(),
        chars.collect::<String>()
    )
}

fn upper_first(name: &str) -> String {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!(
        "{}{}",
        first.to_ascii_uppercase(),
        chars.collect::<String>()
    )
}

/// Convert a camelCase identifier to snake_case.
pub fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (idx, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx != 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Collect local types reachable from callback payloads, including transitive
/// field references, for the Rust WASM and TypeScript host bridges.
pub fn collect_local_bridge_payload_types(definition: &PlatformDefinition) -> BTreeSet<&str> {
    let local: BTreeSet<&str> = definition.types.iter().map(|ty| ty.name.as_str()).collect();
    let mut out = BTreeSet::new();
    if local.contains("ReceivingRegistration") {
        out.insert("ReceivingRegistration");
    }
    for trait_def in &definition.traits {
        for method in &trait_def.methods {
            for param in &method.params {
                collect_local_from_type(&param.type_ref, &local, &mut out);
            }
            match &method.return_shape.inner {
                PlatformInner::Result { ok, .. } | PlatformInner::Plain(ok) => {
                    collect_local_from_type(ok, &local, &mut out);
                }
                PlatformInner::Stream(item) => {
                    collect_local_from_type(stream_item(item), &local, &mut out)
                }
                PlatformInner::Unit | PlatformInner::TraitObject(_) => {}
            }
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        let referenced = definition
            .types
            .iter()
            .filter(|ty| out.contains(ty.name.as_str()))
            .collect::<Vec<_>>();
        for type_def in referenced {
            let before = out.len();
            collect_local_from_type_def(type_def, &local, &mut out);
            changed |= out.len() != before;
        }
    }
    out
}

fn collect_local_from_type_def<'a>(
    type_def: &'a TypeDef,
    local: &BTreeSet<&'a str>,
    out: &mut BTreeSet<&'a str>,
) {
    match &type_def.kind {
        TypeDefKind::Alias(type_ref) => collect_local_from_type(type_ref, local, out),
        TypeDefKind::Struct(fields) => {
            for field in fields {
                collect_local_from_type(&field.type_ref, local, out);
            }
        }
        TypeDefKind::TupleStruct(fields) => {
            for field in fields {
                collect_local_from_type(field, local, out);
            }
        }
        TypeDefKind::Enum(variants) => {
            for variant in variants {
                match &variant.fields {
                    VariantFields::Unit => {}
                    VariantFields::Unnamed(types) => {
                        for ty in types {
                            collect_local_from_type(ty, local, out);
                        }
                    }
                    VariantFields::Named(fields) => {
                        for field in fields {
                            collect_local_from_type(&field.type_ref, local, out);
                        }
                    }
                }
            }
        }
    }
}

fn collect_local_from_type<'a>(
    ty: &'a TypeRef,
    local: &BTreeSet<&'a str>,
    out: &mut BTreeSet<&'a str>,
) {
    match ty {
        TypeRef::Named { name, args } => {
            if local.contains(name.as_str()) {
                out.insert(name);
            }
            for arg in args {
                collect_local_from_type(arg, local, out);
            }
        }
        TypeRef::Vec(inner) | TypeRef::Option(inner) | TypeRef::Array(inner, _) => {
            collect_local_from_type(inner, local, out);
        }
        TypeRef::Tuple(items) => {
            for item in items {
                collect_local_from_type(item, local, out);
            }
        }
        TypeRef::Primitive(_) | TypeRef::Generic(_) | TypeRef::Unit => {}
    }
}
