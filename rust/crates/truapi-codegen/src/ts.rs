//! TypeScript code generation from extracted API definitions.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;
use std::fs;
use std::path::Path;

use anyhow::{Result, bail};
use convert_case::{Case, Casing};
use indoc::{formatdoc, writedoc};

use crate::RESERVED_PROTOCOL_ERROR_TRAIT_ID;
use crate::rustdoc::*;

mod examples;
mod explorer;
mod host_callbacks;
mod playground;
mod visibility;

use visibility::{
    internal_type_names, type_dependencies, type_ref_dependencies, type_ref_uses_hex_string,
    uses_hex_string,
};

pub use examples::generate_client_examples;
pub use explorer::generate_explorer;
pub use host_callbacks::generate as generate_host_callbacks;
pub use playground::generate_playground_services;

#[derive(Default)]
struct CodecContext {
    generic_codecs: HashMap<String, String>,
}

/// How a `TypeRef::Named` resolves its name when rendered to TS.
///
/// `Public` strips the V0N prefix via `public_versioned_type_name` and
/// qualifies every named type with `T.*`. Used by the client/playground/
/// examples generators that emit version-aliased public names (e.g.
/// `T.HostAccountGetRequest`).
#[derive(Clone, Copy, Debug, Default)]
enum NameMode<'a> {
    #[default]
    Public,
    PreserveQualified,
    Generated {
        aliases: &'a BTreeMap<String, String>,
    },
}

fn resolve_named(name: &str, mode: NameMode<'_>) -> String {
    match mode {
        NameMode::Public => public_versioned_type_name(name),
        NameMode::PreserveQualified => name.to_string(),
        NameMode::Generated { aliases } => aliases
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string()),
    }
}

/// Decide how to namespace a resolved type name for `qualified` rendering.
/// `Public` prefixes every name with `T.*`.
fn qualify_named(resolved: &str, mode: NameMode<'_>) -> String {
    match mode {
        NameMode::Public => format!("T.{resolved}"),
        NameMode::PreserveQualified => format!("T.{resolved}"),
        NameMode::Generated { .. } => resolved.to_string(),
    }
}

#[derive(Debug, Clone)]
struct PublicService<'a> {
    trait_def: &'a TraitDef,
}

/// A versioned enum wrapper like `enum HostSignPayloadRequest { V2(Inner) }`,
/// `enum HostCreateTransactionRequest { V2(CreateTransactionRequest) }`,
/// or a multi-version enum `enum HostDevicePermissionRequest { V1(_), V2(_) }`.
///
/// The client generator selects the latest wrapper variant up to its target
/// protocol version, so a V2 package emits V2 wire payloads when available and
/// falls back to V1 for wrappers whose shape did not change.
#[derive(Debug, Clone)]
struct VersionedWrapper {
    variants: BTreeMap<u32, VersionedWrapperVariant>,
}

#[derive(Debug, Clone)]
struct VersionedWrapperVariant {
    version: u32,
    kind: VersionedKind,
}

fn versioned_wrapper_ts_name(name: &str) -> String {
    format!("Versioned{name}")
}

fn version_prefixed_type(name: &str) -> Option<(u32, &str)> {
    let rest = name.strip_prefix('V')?;
    if rest.len() < 3 {
        return None;
    }
    let (version, base) = rest.split_at(2);
    if base.is_empty() {
        return None;
    }
    Some((version.parse().ok()?, base))
}

fn public_versioned_type_name(name: &str) -> String {
    version_prefixed_type(name)
        .map(|(_, base)| base.to_string())
        .unwrap_or_else(|| name.to_string())
}

fn selected_public_aliases(
    api: &ApiDefinition,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    emit_versions: &BTreeMap<String, BTreeSet<u32>>,
    target_version: u32,
) -> BTreeMap<String, String> {
    let mut selected_by_base: BTreeMap<String, (u32, String)> = BTreeMap::new();
    let internal_types = internal_type_names(api);
    // Several wrappers can name one base type at different versions: two still
    // on V1 while a third has reached V2. The unprefixed alias belongs to the
    // newest public version any of them selected. Internal payloads must not
    // take a name used by the public client.
    for (wrapper_name, versions) in emit_versions {
        let Some(wrapper) = wrappers.get(wrapper_name) else {
            continue;
        };
        for version in versions {
            let Some(variant) = wrapper.variants.get(version) else {
                continue;
            };
            let VersionedKind::Tuple(TypeRef::Named { name, args }) = &variant.kind else {
                continue;
            };
            if !args.is_empty() {
                continue;
            }
            let Some((inner_version, base)) = version_prefixed_type(name) else {
                continue;
            };
            match selected_by_base.get_mut(base) {
                Some(selected)
                    if (!internal_types.contains(name), inner_version)
                        > (!internal_types.contains(&selected.1), selected.0) =>
                {
                    *selected = (inner_version, name.clone());
                }
                Some(_) => {}
                None => {
                    selected_by_base.insert(base.to_string(), (inner_version, name.clone()));
                }
            }
        }
    }

    for ty in &api.types {
        let Some((version, base)) = version_prefixed_type(&ty.name) else {
            continue;
        };
        if version > target_version {
            continue;
        }
        if selected_by_base.contains_key(base) {
            continue;
        }
        let entry = selected_by_base
            .entry(base.to_string())
            .or_insert((version, ty.name.clone()));
        if version > entry.0 {
            *entry = (version, ty.name.clone());
        }
    }

    selected_by_base
        .into_iter()
        .map(|(base, (_, original))| (original, base))
        .collect()
}

fn emitted_version_prefixed_types(
    wrappers: &BTreeMap<String, VersionedWrapper>,
    emit_versions: &BTreeMap<String, BTreeSet<u32>>,
    aliases: &BTreeMap<String, String>,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for (wrapper_name, versions) in emit_versions {
        let Some(wrapper) = wrappers.get(wrapper_name) else {
            continue;
        };
        for version in versions {
            let Some(variant) = wrapper.variants.get(version) else {
                continue;
            };
            collect_preserved_version_prefixed_types(&variant.kind, aliases, &mut names);
        }
    }
    names
}

fn preserve_version_prefixed_types_referenced_by_emitted_types(
    api: &ApiDefinition,
    aliases: &BTreeMap<String, String>,
    names: &mut BTreeSet<String>,
) {
    loop {
        let before = names.len();
        for ty in &api.types {
            if version_prefixed_type(&ty.name).is_some()
                && !aliases.contains_key(&ty.name)
                && !names.contains(&ty.name)
            {
                continue;
            }
            preserve_version_prefixed_names(type_dependencies(ty), aliases, names);
        }
        if names.len() == before {
            break;
        }
    }
}

fn preserve_version_prefixed_names(
    dependencies: BTreeSet<String>,
    aliases: &BTreeMap<String, String>,
    names: &mut BTreeSet<String>,
) {
    names.extend(
        dependencies
            .into_iter()
            .filter(|name| version_prefixed_type(name).is_some() && !aliases.contains_key(name)),
    );
}

fn collect_preserved_version_prefixed_types(
    kind: &VersionedKind,
    aliases: &BTreeMap<String, String>,
    names: &mut BTreeSet<String>,
) {
    match kind {
        VersionedKind::Unit => {}
        VersionedKind::Tuple(inner) => {
            preserve_version_prefixed_names(type_ref_dependencies(inner), aliases, names);
        }
    }
}

#[derive(Debug, Clone)]
enum VersionedKind {
    Unit,
    Tuple(TypeRef),
}

fn detect_versioned_wrapper(ty: &TypeDef) -> Option<VersionedWrapper> {
    if !ty.generic_params.is_empty() {
        return None;
    }
    let TypeDefKind::Enum(variants) = &ty.kind else {
        return None;
    };
    if variants.is_empty() || !variants.iter().all(|v| is_versioned_variant_name(&v.name)) {
        return None;
    }
    let mut version_variants = BTreeMap::new();
    for variant in variants {
        let version = version_number(&variant.name)?;
        let kind = match &variant.fields {
            VariantFields::Unit => VersionedKind::Unit,
            VariantFields::Unnamed(types) if types.len() == 1 => {
                VersionedKind::Tuple(types[0].clone())
            }
            _ => return None,
        };
        version_variants.insert(version, VersionedWrapperVariant { version, kind });
    }

    Some(VersionedWrapper {
        variants: version_variants,
    })
}

fn is_versioned_variant_name(name: &str) -> bool {
    version_number(name).is_some()
}

fn version_number(name: &str) -> Option<u32> {
    let rest = name.strip_prefix('V')?;
    if rest.is_empty() {
        return None;
    }
    rest.parse().ok()
}

fn collect_versioned_wrappers(api: &ApiDefinition) -> BTreeMap<String, VersionedWrapper> {
    api.types
        .iter()
        .filter_map(|ty| detect_versioned_wrapper(ty).map(|w| (ty.name.clone(), w)))
        .collect()
}

/// Return the highest protocol version exposed by any versioned wrapper in
/// `api`, falling back to `1` if the API has none. Used as the default for
/// the client target version when the caller did not pass `--client-version`,
/// so an unconfigured codegen run produces a client that speaks the latest
/// wire format the Rust trait surface has shipped.
pub fn latest_wire_version(api: &ApiDefinition) -> u32 {
    collect_versioned_wrappers(api)
        .values()
        .flat_map(|wrapper| wrapper.variants.keys().copied())
        .max()
        .unwrap_or(1)
}

fn validate_versioned_wrapper_shapes(api: &ApiDefinition) -> Result<()> {
    for ty in &api.types {
        let TypeDefKind::Enum(variants) = &ty.kind else {
            continue;
        };
        if variants.is_empty() || !variants.iter().all(|v| is_versioned_variant_name(&v.name)) {
            continue;
        }
        for variant in variants {
            if matches!(variant.fields, VariantFields::Named(_)) {
                bail!(
                    "versioned wrapper `{}` variant `{}` uses named fields; define a request/response struct in the v0x module and wrap it as `{}`(v0x::MyStruct)",
                    ty.name,
                    variant.name,
                    variant.name
                );
            }
        }
    }
    Ok(())
}

fn versioned_wrapper_for<'a>(
    ty: &'a TypeRef,
    wrappers: &'a BTreeMap<String, VersionedWrapper>,
) -> Option<(&'a str, &'a VersionedWrapper)> {
    if let TypeRef::Named { name, args } = ty
        && args.is_empty()
        && let Some(wrapper) = wrappers.get(name)
    {
        return Some((name.as_str(), wrapper));
    }
    None
}

/// Emits a JSDoc block for `docs` at the given indent. No-op when `docs` is
/// `None` so callers can pipe rust doc strings through unconditionally.
///
/// Strips the conventional single space rustdoc preserves after `///` so the
/// emitted JSDoc reads `/** Foo */` rather than `/**  Foo */`. Deeper
/// indentation inside doc blocks is kept verbatim.
fn write_jsdoc(out: &mut String, indent: &str, docs: Option<&str>) {
    let Some(text) = docs else {
        return;
    };
    let text = strip_playground_doc_blocks(text);
    let safe = text.replace("*/", "*\\/");
    let lines: Vec<String> = safe
        .lines()
        .map(|line| {
            let trimmed = line.strip_prefix(' ').unwrap_or(line);
            trimmed.trim_end().to_string()
        })
        .collect();
    if lines.is_empty() {
        return;
    }
    if lines.len() == 1 {
        writeln!(out, "{indent}/** {line} */", line = lines[0]).unwrap();
        return;
    }
    writeln!(out, "{indent}/**").unwrap();
    for line in &lines {
        if line.is_empty() {
            writeln!(out, "{indent} *").unwrap();
        } else {
            writeln!(out, "{indent} * {line}").unwrap();
        }
    }
    writeln!(out, "{indent} */").unwrap();
}

fn strip_playground_doc_blocks(text: &str) -> String {
    let mut out = Vec::new();
    let mut in_typescript_doc_block = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if is_typescript_doc_block_start(trimmed) {
            in_typescript_doc_block = true;
            continue;
        }
        if in_typescript_doc_block && trimmed == "```" {
            in_typescript_doc_block = false;
            continue;
        }
        if !in_typescript_doc_block {
            out.push(line);
        }
    }
    trim_doc_lines(&out).unwrap_or_default()
}

fn is_typescript_doc_block_start(trimmed: &str) -> bool {
    trimmed == "```ts"
}

fn public_services(api: &ApiDefinition) -> Result<Vec<PublicService<'_>>> {
    let trait_defs = api
        .traits
        .iter()
        .map(|trait_def| (trait_def.name.as_str(), trait_def))
        .collect::<HashMap<_, _>>();

    let mut services = Vec::new();
    for name in &api.public_trait_order {
        let Some(trait_def) = trait_defs.get(name.as_str()).copied() else {
            bail!("trait `{name}` appears in `TrUApi` but was not extracted");
        };
        services.push(PublicService { trait_def });
    }

    Ok(services)
}

/// Generates the TypeScript client, types, and barrel files for an extracted
/// API definition into `output_dir`.
pub fn generate(
    api: &ApiDefinition,
    output_dir: &str,
    target_version: u32,
    codec_version: u8,
) -> Result<()> {
    fs::create_dir_all(output_dir)?;
    validate_versioned_wrapper_shapes(api)?;
    let wrappers = collect_versioned_wrappers(api);
    playground::validate_method_examples(api, &wrappers, target_version)?;

    let types_code = generate_types(api, target_version)?;
    fs::write(Path::new(output_dir).join("types.ts"), types_code)?;

    let internal_code = generate_type_bindings(api, target_version, true)?;
    fs::write(Path::new(output_dir).join("internal.ts"), internal_code)?;

    let client_code = generate_client(api, target_version, codec_version)?;
    fs::write(Path::new(output_dir).join("client.ts"), client_code)?;

    let internal_client = generate_client_view(api, target_version, codec_version, true)?;
    fs::write(
        Path::new(output_dir).join("internal-client.ts"),
        internal_client,
    )?;

    let index_code = generate_index();
    fs::write(Path::new(output_dir).join("index.ts"), index_code)?;

    let wire_table_code = generate_wire_table(api, target_version)?;
    fs::write(Path::new(output_dir).join("wire-table.ts"), wire_table_code)?;

    let decode_table_code = generate_decode_table(api, target_version)?;
    fs::write(
        Path::new(output_dir).join("wire-decode.ts"),
        decode_table_code,
    )?;

    Ok(())
}

fn generate_index() -> String {
    "export * from './types.js';\nexport * from './client.js';\n".to_string()
}

fn trim_doc_lines(lines: &[&str]) -> Option<String> {
    let mut start = 0;
    let mut end = lines.len();
    while start < end && lines[start].trim().is_empty() {
        start += 1;
    }
    while end > start && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    if start == end {
        return None;
    }
    Some(
        lines[start..end]
            .iter()
            .map(|line| line.strip_prefix(' ').unwrap_or(line).trim_end())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn ts_string_literal(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization is infallible")
}

fn wire_const_name(trait_name: &str, method_name: &str) -> String {
    format!("{trait_name}_{method_name}").to_case(Case::UpperSnake)
}

/// Sort key for stable, wire-id-ordered method emission shared by the
/// playground and examples submodules.
fn method_wire_sort_id(method: &MethodDef) -> u8 {
    method.wire.id.unwrap_or(u8::MAX)
}

fn generate_wire_table(api: &ApiDefinition, target_version: u32) -> Result<String> {
    let wrappers = collect_versioned_wrappers(api);
    let mut seen: BTreeMap<(u8, u8), String> = BTreeMap::new();
    // Mirrors the Rust emitter's seeding in `rust/wire_table.rs`: trait 255 is
    // reserved for protocol errors and must be refused identically by both, or
    // the two languages would disagree about which addresses are legal.
    let mut seen_traits: BTreeMap<u8, String> = BTreeMap::from([(
        RESERVED_PROTOCOL_ERROR_TRAIT_ID,
        "reserved for protocol errors".to_string(),
    )]);
    let mut constants: Vec<(String, u8, u8, &'static str)> = Vec::new();

    for trait_def in &api.traits {
        // Method-less traits (e.g. the `TrUApi` umbrella trait) own no wire
        // frames and need no trait discriminant.
        if trait_def.methods.is_empty() {
            continue;
        }
        let trait_id = trait_wire_id(trait_def)?;
        if let Some(existing) = seen_traits.insert(trait_id, trait_def.name.clone()) {
            bail!(
                "wire trait id {trait_id} reused: `{existing}` and `{}` collide",
                trait_def.name
            );
        }
        for method in &trait_def.methods {
            let method_id = wire_id_for_method(trait_def, method)?;
            if let Some(existing) = seen.insert((trait_id, method_id), method.name.clone()) {
                bail!(
                    "wire id ({trait_id}, {method_id}) reused: `{existing}` and `{}` collide",
                    method.name
                );
            }
            if !method_is_included(trait_def, method, &wrappers, target_version)? {
                continue;
            }
            let wire_kind = match method.kind {
                MethodKind::Request => "request",
                MethodKind::Subscription => "subscription",
            };
            constants.push((
                wire_const_name(&trait_def.name, &method.name),
                trait_id,
                method_id,
                wire_kind,
            ));
        }
    }

    constants.sort_by_key(|(_, trait_id, method_id, _)| (*trait_id, *method_id));

    let mut out = String::new();
    writedoc!(
        out,
        r#"
        // Auto-generated by truapi-codegen. Do not edit.

        import type {{ MethodIds }} from '../transport.js';

        // Wire-protocol (trait, method) discriminant pairs. One id addresses
        // a method regardless of shape; a frame's own third byte
        // (`messageType`) names which leg of the exchange it carries
        // (Request/Response, or a subscription's Start/Receive/Interrupt/
        // Stop). `kind` says whether a method's legs follow the
        // request/response or subscription shape, the one piece of
        // information a payload-blind reader (e.g. a wire debugger) needs to
        // interpret `messageType` without decoding the payload. Trait and
        // method ordering are part of the protocol; only ever append within a
        // trait or explicitly reserve gaps.

        "#
    )
    .unwrap();
    for (name, trait_id, method_id, wire_kind) in constants {
        out.push('\n');
        out.push_str(&formatdoc! {r#"
            export const {name} = {{
              trait: {trait_id},
              method: {method_id},
              kind: "{wire_kind}",
            }} as const satisfies MethodIds;
        "#});
    }

    Ok(out)
}

/// The trait's wire discriminant. Every API trait must carry a
/// `#[wire_trait(id = N)]` annotation; 255 is reserved for protocol errors
/// (that one is caught as a collision against the seeded reservation, not
/// here).
fn trait_wire_id(trait_def: &TraitDef) -> Result<u8> {
    trait_def.wire_trait_id.ok_or_else(|| {
        anyhow::anyhow!(
            "trait `{}` is missing #[wire_trait(id = N)] annotation",
            trait_def.name
        )
    })
}

/// One row of the wire contract: a frame id, its method leg, whether the method
/// is host-initiated, and the structural signature of the payload that frame
/// carries.
type WireIdRow = (u8, u8, String, bool, String);

/// Every wire address (sorted by `(trait, method)`) with the facts above. The
/// one iteration the schema-hash fingerprint is derived from, so the
/// fingerprint tracks exactly what the wire table publishes.
///
/// `host_initiated` is in the row because it decides WHICH SIDE sends a frame
/// id. A host and product that disagree on it both send `start` on the same id
/// and neither answers, a mismatch the fingerprint has to catch.
fn wire_id_rows(api: &ApiDefinition, target_version: u32) -> Result<Vec<WireIdRow>> {
    let wrappers = collect_versioned_wrappers(api);
    let types = types_by_name(api);
    // Seed the reserved address so the fingerprint moves if it ever moves.
    // `generate_wire_table` reserves it in its own collision map, but that map is
    // not what the hash folds over, so without this row the reserved address could
    // be reassigned and every already-built debugger would keep confirming the
    // table. It carries no payload and no method, so its facts are fixed.
    let mut seen: BTreeMap<(u8, u8), (String, bool, String)> = BTreeMap::from([(
        (
            RESERVED_PROTOCOL_ERROR_TRAIT_ID,
            RESERVED_PROTOCOL_ERROR_TRAIT_ID,
        ),
        ("reserved::protocol_error".to_string(), false, String::new()),
    )]);
    for trait_def in &api.traits {
        // Method-less traits (e.g. the `TrUApi` umbrella trait) own no wire
        // frames and need no trait discriminant, matching `generate_wire_table`.
        if trait_def.methods.is_empty() {
            continue;
        }
        let trait_id = trait_wire_id(trait_def)?;
        for method in &trait_def.methods {
            if !method_is_included(trait_def, method, &wrappers, target_version)? {
                continue;
            }
            let method_id = wire_id_for_method(trait_def, method)?;
            let payload = method_payload_signature(method, &types);
            // Qualify the tag with the trait. The tag is what the schema hash folds
            // in, and the debugger's method label is derived from the generated
            // const name (`LOCAL_STORAGE_WRITE` -> `localStorage.write`), which is
            // trait-qualified. Hashing the bare method name lets a trait rename, or
            // a method moving between traits, keep the same fingerprint: the host
            // then stamps a hash the debugger affirmatively confirms and decode
            // proceeds under the wrong label, which is the exact failure this hash
            // exists to refuse.
            let qualified = format!("{}::{}", trait_def.name, method.name);
            if let Some((existing, _, _)) = seen.insert(
                (trait_id, method_id),
                (qualified.clone(), method.wire.host_initiated, payload),
            ) {
                bail!(
                    "wire id ({trait_id}, {method_id}) reused: `{existing}` and \
                     `{qualified}` collide"
                );
            }
        }
    }
    Ok(seen
        .into_iter()
        .map(|((trait_id, method_id), (tag, host_initiated, payload))| {
            (trait_id, method_id, tag, host_initiated, payload)
        })
        .collect())
}

/// Index the API's user-defined types by their emitted name, so a signature walk
/// can resolve a [`TypeRef::Named`] to its actual shape.
fn types_by_name(api: &ApiDefinition) -> HashMap<&str, &TypeDef> {
    // Framework types are included even though they are never emitted: their
    // shape is still on the wire. `CallError` is the one that matters - it wraps
    // every error leg, so its variant list is the discriminant of every error
    // response, and leaving it out let a variant be inserted (renumbering every
    // discriminant on every error) without moving the fingerprint at all.
    api.types
        .iter()
        .chain(api.framework_types.iter())
        .map(|def| (def.name.as_str(), def))
        .collect()
}

/// Structural signature of everything a method puts on the wire: its parameters
/// (the request/start payload) and its return shape (the response/item payload).
///
/// Folded into the wire schema hash so the fingerprint moves when a payload's
/// *layout* changes, not only when a frame id or method name does.
fn method_payload_signature(method: &MethodDef, types: &HashMap<&str, &TypeDef>) -> String {
    let mut out = String::new();
    for param in &method.params {
        let sig = type_signature(&param.type_ref, types, &mut Vec::new());
        let _ = write!(out, "{}:{sig},", param.name);
    }
    out.push_str("->");
    match &method.return_type {
        ReturnType::Result { ok, err } => {
            let _ = write!(
                out,
                "res<{},{}>",
                type_signature(ok, types, &mut Vec::new()),
                type_signature(err, types, &mut Vec::new())
            );
        }
        ReturnType::Subscription { item, interrupt } => {
            let _ = write!(
                out,
                "sub<{},{}>",
                type_signature(item, types, &mut Vec::new()),
                type_signature(interrupt, types, &mut Vec::new())
            );
        }
    }
    out
}

/// Canonical structural rendering of a type: field order and field types for a
/// struct, positional variant indices and payloads for an enum, resolved
/// transitively.
///
/// Two layouts that encode differently under SCALE cannot render the same
/// string: field order, field types, variant order, and arity all appear. A type
/// this crate does not own (external or generic) degrades to its name, which is
/// the most that is knowable from rustdoc. `seen` guards recursive types.
fn type_signature(
    type_ref: &TypeRef,
    types: &HashMap<&str, &TypeDef>,
    seen: &mut Vec<String>,
) -> String {
    match type_ref {
        TypeRef::Primitive(name) => name.clone(),
        TypeRef::Unit => "()".to_string(),
        TypeRef::Generic(name) => format!("generic:{name}"),
        TypeRef::Vec(inner) => format!("vec<{}>", type_signature(inner, types, seen)),
        TypeRef::Option(inner) => format!("opt<{}>", type_signature(inner, types, seen)),
        TypeRef::Array(inner, len) => {
            format!("[{};{len}]", type_signature(inner, types, seen))
        }
        TypeRef::Tuple(items) => {
            let inner: Vec<String> = items
                .iter()
                .map(|item| type_signature(item, types, seen))
                .collect();
            format!("({})", inner.join(","))
        }
        TypeRef::Named { name, args } => {
            let rendered_args: Vec<String> = args
                .iter()
                .map(|arg| type_signature(arg, types, seen))
                .collect();
            let suffix = if rendered_args.is_empty() {
                String::new()
            } else {
                format!("<{}>", rendered_args.join(","))
            };
            // A type already on the walk stack is recursive; naming it closes the
            // cycle without losing that the edge exists.
            if seen.iter().any(|entry| entry == name) {
                return format!("rec:{name}{suffix}");
            }
            let Some(def) = types.get(name.as_str()) else {
                // Degrading silently to the bare name is what let a payload's
                // shape change without moving the fingerprint - the type's own
                // fields or variants simply stop being hashed. Marking it keeps
                // the blind spot visible in the canonical string, and
                // `every_wire_reachable_type_resolves` fails the build if a new
                // one ever appears.
                return format!("UNRESOLVED<{name}>{suffix}");
            };
            seen.push(name.clone());
            let body = match &def.kind {
                TypeDefKind::Alias(inner) => {
                    format!("={}", type_signature(inner, types, seen))
                }
                TypeDefKind::Struct(fields) => {
                    let rendered: Vec<String> = fields
                        .iter()
                        .map(|field| {
                            format!(
                                "{}:{}",
                                field.name,
                                type_signature(&field.type_ref, types, seen)
                            )
                        })
                        .collect();
                    format!("{{{}}}", rendered.join(","))
                }
                TypeDefKind::TupleStruct(items) => {
                    let rendered: Vec<String> = items
                        .iter()
                        .map(|item| type_signature(item, types, seen))
                        .collect();
                    format!("({})", rendered.join(","))
                }
                TypeDefKind::Enum(variants) => {
                    let rendered: Vec<String> = variants
                        .iter()
                        .enumerate()
                        .map(|(index, variant)| {
                            let payload = match &variant.fields {
                                VariantFields::Unit => String::new(),
                                VariantFields::Unnamed(items) => {
                                    let inner: Vec<String> = items
                                        .iter()
                                        .map(|item| type_signature(item, types, seen))
                                        .collect();
                                    format!("({})", inner.join(","))
                                }
                                VariantFields::Named(fields) => {
                                    let inner: Vec<String> = fields
                                        .iter()
                                        .map(|field| {
                                            format!(
                                                "{}:{}",
                                                field.name,
                                                type_signature(&field.type_ref, types, seen)
                                            )
                                        })
                                        .collect();
                                    format!("{{{}}}", inner.join(","))
                                }
                            };
                            // The SCALE discriminant is the explicit
                            // `#[codec(index = N)]` when the variant carries one,
                            // and the positional index otherwise. Hash whichever
                            // actually ships: fingerprinting position alone is
                            // blind to a renumbering that leaves declaration order
                            // untouched, which is how RFC-0024 moved `Rejected`
                            // from `0x02` to `0x04` without any signal.
                            let discriminant = variant
                                .codec_index
                                .map(|explicit| explicit.to_string())
                                .unwrap_or_else(|| index.to_string());
                            format!("{discriminant}:{}{payload}", variant.name)
                        })
                        .collect();
                    format!("|{}|", rendered.join(";"))
                }
            };
            seen.pop();
            format!("{name}{suffix}{body}")
        }
    }
}

/// A stable fingerprint of the wire contract: every frame id and the method leg
/// it resolves to, folded together with the codec version. Two builds whose
/// frame tables differ - a reassigned id, or a renamed, added or removed
/// method - produce different
/// hashes even when the handshake `codec_version` is unchanged, which is the
/// case the coarse codec number cannot see. Emitted as `TRUAPI_WIRE_SCHEMA_HASH`
/// on both the TS and Rust sides so a host stamps it on every debug envelope and
/// the debugger refuses to decode a frame whose contract differs from its own.
pub fn wire_schema_hash(
    api: &ApiDefinition,
    target_version: u32,
    codec_version: u8,
) -> Result<String> {
    let mut canonical = format!("codec={codec_version}\n");
    let mut unresolved: BTreeSet<String> = BTreeSet::new();
    for (trait_id, method_id, tag, host_initiated, payload) in wire_id_rows(api, target_version)? {
        let initiator = u8::from(host_initiated);
        for marker in payload.split("UNRESOLVED<").skip(1) {
            unresolved.insert(marker.chars().take_while(|c| *c != '>').collect());
        }
        canonical.push_str(&format!(
            "{trait_id}.{method_id}:{tag}:{initiator}:{payload}\n"
        ));
    }
    // Fail the BUILD, not a test. A type that does not resolve contributes only
    // its name, so its own fields or variants stop being fingerprinted and can
    // change undetected - `CallError` sat on every error leg exactly that way,
    // and inserting a variant renumbered every error discriminant while the hash
    // and the whole generated tree stayed byte-identical. Enforcing it here means
    // a future addition to the extractor's skip list cannot re-open the hole, and
    // does not depend on a test being wired up to notice.
    if !unresolved.is_empty() {
        bail!(
            "wire schema hash cannot see the shape of {unresolved:?}: these types are \
             reachable from a wire payload but are not in the API definition, so a \
             change to their fields or variants would not move the fingerprint. Add \
             them to `ApiDefinition::framework_types` rather than letting the \
             signature degrade to a bare name."
        );
    }
    // FNV-1a 64-bit: deterministic across platforms and Rust versions (unlike
    // `DefaultHasher`), dependency-free, and ample for a contract fingerprint.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in canonical.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Ok(format!("{hash:016x}"))
}

fn method_is_included(
    trait_def: &TraitDef,
    method: &MethodDef,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<bool> {
    wire_id_for_method(trait_def, method)?;

    let wrapper_names = method_versioned_wrappers(method, wrappers);
    Ok(
        wrapper_names.is_empty()
            || method_wire_version(method, wrappers, target_version)?.is_some(),
    )
}

fn wire_id_for_method(trait_def: &TraitDef, method: &MethodDef) -> Result<u8> {
    method.wire.id.ok_or_else(|| {
        anyhow::anyhow!(
            "method `{}::{}` is missing #[wire(id = N)] annotation",
            trait_def.name,
            method.name
        )
    })
}

/// Picks the wrapper variant the generated client emits on the wire for a
/// given method. Returns the highest variant supported by every wrapper the
/// method touches and that is ≤ `target_version`. Returns `None` when no
/// shared variant exists at or below the cap (the method is not exposed by
/// the client).
///
/// Picking the **highest** variant exposes the newest request/response shape
/// the host is known to support. Hosts that only implement an older codec
/// version still receive a wire envelope they understand because every
/// wrapper keeps each `Vn` variant at `#[codec(index = n - 1)]`.
fn method_wire_version(
    method: &MethodDef,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<Option<u32>> {
    let wrapper_names = method_versioned_wrappers(method, wrappers);
    if wrapper_names.is_empty() {
        return Ok(None);
    }

    let mut candidates: Option<Vec<u32>> = None;
    for wrapper_name in wrapper_names {
        let wrapper = wrappers
            .get(&wrapper_name)
            .expect("method_versioned_wrappers only returns known wrappers");
        let versions = wrapper
            .variants
            .keys()
            .filter(|version| **version <= target_version)
            .copied()
            .collect::<Vec<_>>();
        candidates = Some(match candidates {
            Some(current) => current
                .into_iter()
                .filter(|version| versions.contains(version))
                .collect(),
            None => versions,
        });
    }

    Ok(candidates.and_then(|versions| versions.into_iter().max()))
}

/// For each versioned wrapper, the set of wire versions the generated client
/// actually emits. Each method picks one wire version via [`method_wire_version`];
/// every wrapper it touches gets that version recorded here. Wrappers that no
/// included method references end up absent from the map and can be elided
/// from the emitted types altogether.
fn versioned_wrapper_emit_versions(
    api: &ApiDefinition,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<BTreeMap<String, BTreeSet<u32>>> {
    let mut emit: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    for trait_def in &api.traits {
        for method in &trait_def.methods {
            if !method_is_included(trait_def, method, wrappers, target_version)? {
                continue;
            }
            let Some(wire_version) = method_wire_version(method, wrappers, target_version)? else {
                continue;
            };
            for wrapper_name in method_versioned_wrappers(method, wrappers) {
                emit.entry(wrapper_name).or_default().insert(wire_version);
            }
        }
    }
    Ok(emit)
}

fn method_versioned_wrappers(
    method: &MethodDef,
    wrappers: &BTreeMap<String, VersionedWrapper>,
) -> Vec<String> {
    let mut names = Vec::new();
    for param in &method.params {
        collect_type_versioned_wrappers(&param.type_ref, wrappers, &mut names);
    }
    match &method.return_type {
        ReturnType::Result { ok, err } => {
            collect_type_versioned_wrappers(ok, wrappers, &mut names);
            collect_type_versioned_wrappers(
                call_error_inner(err).unwrap_or(err),
                wrappers,
                &mut names,
            );
        }
        ReturnType::Subscription { item, interrupt } => {
            collect_type_versioned_wrappers(item, wrappers, &mut names);
            collect_type_versioned_wrappers(
                call_error_inner(interrupt).unwrap_or(interrupt),
                wrappers,
                &mut names,
            );
        }
    }
    names.sort();
    names.dedup();
    names
}

fn call_error_inner(ty: &TypeRef) -> Option<&TypeRef> {
    match ty {
        TypeRef::Named { name, args } if name == "CallError" && args.len() == 1 => Some(&args[0]),
        _ => None,
    }
}

fn collect_type_versioned_wrappers(
    ty: &TypeRef,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    names: &mut Vec<String>,
) {
    match ty {
        TypeRef::Named { name, args } => {
            if args.is_empty() && wrappers.contains_key(name) {
                names.push(name.clone());
            }
            for arg in args {
                collect_type_versioned_wrappers(arg, wrappers, names);
            }
        }
        TypeRef::Vec(inner) | TypeRef::Option(inner) | TypeRef::Array(inner, _) => {
            collect_type_versioned_wrappers(inner, wrappers, names);
        }
        TypeRef::Tuple(items) => {
            for item in items {
                collect_type_versioned_wrappers(item, wrappers, names);
            }
        }
        TypeRef::Primitive(_) | TypeRef::Generic(_) | TypeRef::Unit => {}
    }
}

fn generate_types(api: &ApiDefinition, target_version: u32) -> Result<String> {
    generate_type_bindings(api, target_version, false)
}

fn binding_dependencies(
    ty: &TypeDef,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    emit_versions: &BTreeMap<String, BTreeSet<u32>>,
) -> (BTreeSet<String>, bool) {
    let Some(wrapper) = wrappers.get(&ty.name) else {
        return (type_dependencies(ty), uses_hex_string(ty));
    };
    let selected = emit_versions.get(&ty.name);
    let mut names = BTreeSet::new();
    let mut needs_hex_string = false;
    for variant in wrapper.variants.values() {
        if selected.is_some_and(|versions| !versions.contains(&variant.version)) {
            continue;
        }
        if let VersionedKind::Tuple(inner) = &variant.kind {
            names.extend(type_ref_dependencies(inner));
            needs_hex_string |= type_ref_uses_hex_string(inner);
        }
    }
    (names, needs_hex_string)
}

fn generate_type_bindings(
    api: &ApiDefinition,
    target_version: u32,
    internal: bool,
) -> Result<String> {
    let wrappers = collect_versioned_wrappers(api);
    let emit_versions = versioned_wrapper_emit_versions(api, &wrappers, target_version)?;
    let aliases = selected_public_aliases(api, &wrappers, &emit_versions, target_version);
    let mut preserved_version_prefixed_types =
        emitted_version_prefixed_types(&wrappers, &emit_versions, &aliases);
    preserve_version_prefixed_types_referenced_by_emitted_types(
        api,
        &aliases,
        &mut preserved_version_prefixed_types,
    );
    let internal_types = internal_type_names(api);
    let types: Vec<_> = api
        .types
        .iter()
        .filter(|ty| {
            internal_types.contains(&ty.name) == internal
                && (version_prefixed_type(&ty.name).is_none()
                    || aliases.contains_key(&ty.name)
                    || preserved_version_prefixed_types.contains(&ty.name))
        })
        .collect();
    let dependencies: Vec<_> = types
        .iter()
        .map(|ty| binding_dependencies(ty, &wrappers, &emit_versions))
        .collect();

    let mut out = String::from("// Auto-generated by truapi-codegen. Do not edit.\n\n");
    if !types.is_empty() {
        writeln!(out, "import * as S from '../scale.js';").unwrap();
    }
    if dependencies
        .iter()
        .any(|(_, needs_hex_string)| *needs_hex_string)
    {
        writeln!(out, "import type {{ HexString }} from '../scale.js';").unwrap();
    }
    if internal {
        let definitions = types_by_name(api);
        let imports = dependencies
            .iter()
            .flat_map(|(names, _)| names)
            .filter(|name| !internal_types.contains(*name))
            .filter_map(|name| definitions.get(name.as_str()).copied())
            .map(|ty| emitted_type_name(ty, &emit_versions, &aliases))
            .collect::<BTreeSet<_>>();
        if !imports.is_empty() {
            writeln!(
                out,
                "import {{ {} }} from './types.js';",
                imports.into_iter().collect::<Vec<_>>().join(", ")
            )
            .unwrap();
        }
        writeln!(out, "export * from './types.js';").unwrap();
    }
    writeln!(out).unwrap();
    for ty in types {
        write_type_definition(&mut out, ty, &emit_versions, &aliases)?;
        writeln!(out).unwrap();
        write_codec_definition(&mut out, ty, &emit_versions, &aliases)?;
        writeln!(out).unwrap();
    }
    Ok(out)
}

fn generate_client(api: &ApiDefinition, target_version: u32, codec_version: u8) -> Result<String> {
    generate_client_view(api, target_version, codec_version, false)
}

fn generate_client_view(
    api: &ApiDefinition,
    target_version: u32,
    codec_version: u8,
    internal: bool,
) -> Result<String> {
    validate_versioned_wrapper_shapes(api)?;
    let wrappers = collect_versioned_wrappers(api);
    let mut services = Vec::new();
    let mut uses_hex_string = false;
    for service in public_services(api)? {
        let methods = included_wire_methods(service.trait_def, &wrappers, target_version)?
            .into_iter()
            .filter(|method| method.wire.internal == internal)
            .collect::<Vec<_>>();
        for method in &methods {
            if let Some(version) = method_wire_version(method, &wrappers, target_version)? {
                uses_hex_string |= method_versioned_wrappers(method, &wrappers)
                    .iter()
                    .filter_map(|name| wrappers[name].variants.get(&version))
                    .any(|variant| match &variant.kind {
                        VersionedKind::Tuple(inner) => type_ref_uses_hex_string(inner),
                        VersionedKind::Unit => false,
                    });
            }
        }
        if !methods.is_empty() {
            services.push((service.trait_def, methods));
        }
    }
    let has_subscriptions = services.iter().any(|(_, methods)| {
        methods
            .iter()
            .any(|method| method.kind == MethodKind::Subscription)
    });
    let has_host_initiated = services
        .iter()
        .any(|(_, methods)| methods.iter().any(|method| method.wire.host_initiated));
    let hex_import = if uses_hex_string {
        "import type { HexString } from '../scale.js';"
    } else {
        ""
    };
    let registration_import = if has_host_initiated {
        "HostInitiatedSubscriptionRegistration, "
    } else {
        ""
    };
    let method_ids_import = if has_subscriptions { "MethodIds, " } else { "" };

    let schema_hash = wire_schema_hash(api, target_version, codec_version)?;
    let types_module = if internal { "internal" } else { "types" };
    let mut out = String::new();
    writedoc!(
        out,
        r#"
        // Auto-generated by truapi-codegen. Do not edit.

        import {{ ResultAsync, type Result }} from 'neverthrow';
        import * as S from '../scale.js';
        {hex_import}
        import {{ SubscriptionError }} from '../transport.js';
        import type {{ CallOptions, HostInitiatedSubscriptionHandler, {registration_import}{method_ids_import}ObservableLike, Observer, Subscription, TrUApiTransport }} from '../transport.js';
        import * as T from './{types_module}.js';
        import * as W from './wire-table.js';

        export {{ ResultAsync, SubscriptionError }};
        export type {{ CallOptions, HostInitiatedSubscriptionHandler, ObservableLike, Observer, Result, Subscription, TrUApiTransport }};
        export const TRUAPI_VERSION = {target_version} as const;
        export const TRUAPI_CODEC_VERSION = {codec_version} as const;
        export const TRUAPI_WIRE_SCHEMA_HASH = "{schema_hash}" as const;

        "#
    )
    .unwrap();
    if has_subscriptions {
        write_subscription_helpers(&mut out);
        write_observable_helper(&mut out);
    }

    for (trait_def, methods) in &services {
        let public_docs = trait_def.public_docs();
        write_jsdoc(&mut out, "", public_docs.as_deref());
        let export = if internal { "" } else { "export " };
        writeln!(out, "{export}class {}Client {{", trait_def.name).unwrap();
        let uses_transport = methods.iter().any(|method| !method.wire.host_initiated);
        if uses_transport {
            writeln!(out, "  readonly #transport: TrUApiTransport;").unwrap();
        }
        for method in methods
            .iter()
            .copied()
            .filter(|method| method.wire.host_initiated)
        {
            emit_host_initiated_field(&mut out, method, &wrappers, target_version)?;
        }
        writeln!(out, "  constructor(transport: TrUApiTransport) {{").unwrap();
        if uses_transport {
            writeln!(out, "    this.#transport = transport;").unwrap();
        }
        for method in methods
            .iter()
            .copied()
            .filter(|method| method.wire.host_initiated)
        {
            emit_host_initiated_registration(
                &mut out,
                api,
                trait_def,
                method,
                &wrappers,
                target_version,
            )?;
        }
        writeln!(out, "  }}\n").unwrap();

        for method in methods {
            emit_method(&mut out, api, trait_def, method, &wrappers, target_version)?;
            writeln!(out).unwrap();
        }
        writeln!(out, "}}\n").unwrap();
        if internal {
            writeln!(out, "Object.freeze({}Client.prototype);\n", trait_def.name).unwrap();
        }
    }

    let client_type = if internal {
        "InternalTrUApiClient"
    } else {
        "TrUApiClient"
    };
    writeln!(out, "export interface {client_type} {{").unwrap();
    for (trait_def, _) in &services {
        let field = to_camel_case(&trait_def.name);
        let namespace_type = format!("{}Client", trait_def.name);
        let namespace_type = if internal {
            format!("Readonly<{namespace_type}>")
        } else {
            namespace_type
        };
        writeln!(out, "  readonly {field}: {namespace_type};").unwrap();
    }
    writeln!(out, "}}\n").unwrap();
    if !internal {
        writeln!(out, "export type Client = TrUApiClient;\n").unwrap();
    }
    let factory = if internal {
        "createInternalClient"
    } else {
        "createClient"
    };
    let freeze = if internal { "Object.freeze(" } else { "" };
    let close = if internal { ")" } else { "" };
    writedoc!(
        out,
        r#"
        /** Creates the generated client facade by binding each service namespace to the
         * shared transport instance. */
        export function {factory}(transport: TrUApiTransport): {client_type} {{
          return {freeze}{{
        "#
    )
    .unwrap();
    for (trait_def, _) in &services {
        let field = to_camel_case(&trait_def.name);
        writeln!(
            out,
            "    {field}: {freeze}new {name}Client(transport){close},",
            name = trait_def.name
        )
        .unwrap();
    }
    writedoc!(
        out,
        r#"
          }}{close};
        }}
        "#
    )
    .unwrap();
    Ok(out)
}

fn write_subscription_helpers(out: &mut String) {
    writedoc!(
        out,
        r#"
        function toSubscriptionError<Reason = never>(error: unknown): SubscriptionError<Reason> {{
          if (error instanceof SubscriptionError) return error as SubscriptionError<Reason>;
          const cause = error instanceof Error ? error : new Error(String(error));
          return new SubscriptionError(cause.message, {{ cause }});
        }}

        // Interrupt payload sent (with messageType Interrupt) when a
        // host-initiated start arrives the product cannot serve, declining
        // it: Err(CallError::HostFailure with reason "unavailable").
        // HostFailure's payload doesn't depend on the method's own domain
        // error type, so this fixed frame is valid for every method
        // regardless of what D in CallError<D> decodes to.
        const HOST_INITIATED_DECLINE_PAYLOAD = new Uint8Array([
          1, 4, 44, 117, 110, 97, 118, 97, 105, 108, 97, 98, 108, 101,
        ]);
        // Items buffered per host-initiated stream while the product has no
        // handler installed yet.
        const HOST_INITIATED_BUFFER_CAPACITY = 64;

        // The Interrupt leg carries Result<(), CallError<Err>>: Ok(()) ends
        // the stream normally, Err(reason) ends it with the method's own
        // interrupt value.
        function interruptDecoder<Reason>(
          reason: S.Codec<Reason>,
        ): (payload: Uint8Array) => Reason | undefined {{
          const codec = S.Result(S._void, reason);
          return (payload) => {{
            const decoded = codec.dec(payload);
            return decoded.success ? undefined : decoded.value;
          }};
        }}

        function interruptEncoder<Reason>(
          reason: S.Codec<Reason>,
        ): (value?: Reason) => Uint8Array {{
          const codec = S.Result(S._void, reason);
          return (value) =>
            codec.enc(
              value === undefined
                ? {{ success: true, value: undefined }}
                : {{ success: false, value }},
            );
        }}

        "#
    )
    .unwrap();
}

/// Generates the dev-only wire decode table (`wire-decode.ts`): a map from a
/// wire address (`trait * 256 + method`) to a decoder keyed by that frame's
/// own `messageType` byte (`Request`/`Start` = 0, `Response`/`Receive` = 1,
/// `Interrupt` = 2, `Stop` = 3), so a debugger reads the wire's own leg
/// marker instead of having to decode a payload to learn its shape.
fn generate_decode_table(api: &ApiDefinition, target_version: u32) -> Result<String> {
    let ctx = CodecContext::default();
    let wrappers = collect_versioned_wrappers(api);
    let services = public_services(api)?;

    // (numeric key, emitted table lines) pairs, sorted for a stable,
    // wire-ordered file that matches the wire-table layout.
    let mut entries: Vec<(u32, Vec<String>)> = Vec::new();

    for service in &services {
        let trait_def = service.trait_def;
        for method in included_wire_methods(trait_def, &wrappers, target_version)? {
            let wire_const = wire_const_name(&trait_def.name, &method.name);
            let method_id = wire_id_for_method(trait_def, method)?;
            let trait_id = trait_wire_id(trait_def)?;
            let key = u32::from(trait_id) * 256 + u32::from(method_id);

            let request_decoder = match request_wrapper_name(method, &wrappers) {
                Some(name) => format!(
                    "(payload) => T.{}.dec(payload)",
                    versioned_wrapper_ts_name(name)
                ),
                None => "() => undefined".to_string(),
            };

            let mut lines = Vec::new();
            match (&method.kind, &method.return_type) {
                (MethodKind::Request, ReturnType::Result { ok, err }) => {
                    let response_codec = leg_codec_expr(ok, &wrappers)?;
                    let error_codec = leg_error_codec_expr(err, &wrappers, &ctx)?;
                    lines.push(format!("    0: {request_decoder},"));
                    lines.push(format!(
                        "    1: (payload) => S.Result({response_codec}, {error_codec}).dec(payload),"
                    ));
                }
                (MethodKind::Subscription, ReturnType::Subscription { item, interrupt }) => {
                    let item_codec = leg_codec_expr(item, &wrappers)?;
                    let error_codec = leg_error_codec_expr(interrupt, &wrappers, &ctx)?;
                    lines.push(format!("    0: {request_decoder},"));
                    lines.push(format!("    1: (payload) => {item_codec}.dec(payload),"));
                    lines.push(format!(
                        "    2: (payload) => S.Result(S._void, {error_codec}).dec(payload),"
                    ));
                    lines.push("    3: () => undefined,".to_string());
                }
                (kind, return_type) => {
                    bail!(
                        "Generator internal mismatch for method `{}`: kind {:?} does not match return type {:?}",
                        method.name,
                        kind,
                        return_type
                    );
                }
            }

            entries.push((
                key,
                vec![
                    format!("  [W.{wire_const}.trait * 256 + W.{wire_const}.method]: {{"),
                    lines.join("\n"),
                    "  },".to_string(),
                ],
            ));
        }
    }

    entries.sort_by_key(|(key, _)| *key);

    let mut out = String::new();
    writedoc!(
        out,
        r#"
        // Auto-generated by truapi-codegen. Do not edit.

        import * as S from '../scale.js';
        import * as T from './internal.js';
        import * as W from './wire-table.js';

        /** Dev-only: decode a wire frame's SCALE payload, keyed by `trait * 256 +
         *  method` and then by that frame's own `messageType` byte. Unknown
         *  addresses or message types are absent (caller falls back to bytes). */
        export const WIRE_DECODE_TABLE: Record<
          number,
          Record<number, (payload: Uint8Array) => unknown>
        > = {{
        "#
    )
    .unwrap();
    for (_, lines) in &entries {
        for line in lines {
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push_str("};\n");

    Ok(out)
}

fn write_observable_helper(out: &mut String) {
    writedoc!(
        out,
        r#"
        // ES Observable interop key (rxjs reads Symbol.observable, falling
        // back to "@@observable" on platforms without the well-known symbol).
        const OBSERVABLE_INTEROP: symbol | string =
          (typeof Symbol === "function" && (Symbol as {{ observable?: symbol }}).observable) ||
          "@@observable";

        function createObservable<Item, Reason = never>({{
          transport,
          ids,
          payload,
          decodeItem,
          decodeInterrupt,
          onSubscribe,
        }}: {{
          transport: TrUApiTransport;
          ids: MethodIds;
          payload: Uint8Array;
          decodeItem: (payload: Uint8Array) => Item;
          // `undefined` signals a clean, error-free completion (the wire
          // envelope's `Interrupt(Ok(()))`), distinct from not being able to
          // observe a typed reason at all, where `decodeInterrupt` itself is
          // omitted.
          decodeInterrupt?: (payload: Uint8Array) => Reason | undefined;
          onSubscribe?: (subscription: Subscription) => {{ unsubscribe(): void }};
        }}): ObservableLike<Item, Reason> {{
          const observable: ObservableLike<Item, Reason> = {{
            subscribe(observer: Partial<Observer<Item, Reason>> = {{}}): Subscription {{
              let closed = false;
              let raw: Subscription | undefined;
              let forwarding: {{ unsubscribe(): void }} | undefined;

              const stopForwarding = () => {{
                const active = forwarding;
                forwarding = undefined;
                active?.unsubscribe();
              }};

              const fail = (error: unknown, stop = true) => {{
                if (closed) return;
                closed = true;
                try {{
                  stopForwarding();
                  if (stop) raw?.unsubscribe();
                }} finally {{
                  observer.error?.(toSubscriptionError<Reason>(error));
                }}
              }};

              raw = transport.subscribeRaw({{
                ids,
                payload,
                onReceive: (payload) => {{
                  if (closed) return;
                  try {{
                    observer.next?.(decodeItem(payload));
                  }} catch (error) {{
                    fail(error);
                  }}
                }},
                onInterrupt: (payload) => {{
                  if (closed) return;
                  if (decodeInterrupt) {{
                    let reason: unknown;
                    try {{
                      reason = decodeInterrupt(payload);
                    }} catch (error) {{
                      fail(error, false);
                      return;
                    }}
                    if (reason === undefined) {{
                      closed = true;
                      stopForwarding();
                      observer.complete?.();
                      return;
                    }}
                    fail(new SubscriptionError("Subscription interrupted", {{ reason }}), false);
                    return;
                  }}
                  closed = true;
                  stopForwarding();
                  observer.complete?.();
                }},
                onClose: fail,
              }});

              if (!closed && onSubscribe) {{
                try {{
                  forwarding = onSubscribe(raw);
                }} catch (error) {{
                  raw.unsubscribe();
                  throw error;
                }}
                if (closed) stopForwarding();
              }}

              return {{
                get subscriptionId() {{
                  return raw?.subscriptionId ?? "";
                }},
                unsubscribe: () => {{
                  if (closed) return;
                  closed = true;
                  stopForwarding();
                  raw?.unsubscribe();
                }},
              }};
            }},
            [OBSERVABLE_INTEROP as typeof Symbol.observable]() {{
              return observable;
            }},
          }};
          return observable;
        }}

        "#
    )
    .unwrap();
}

fn included_methods<'a>(
    trait_def: &'a TraitDef,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<Vec<&'a MethodDef>> {
    Ok(included_wire_methods(trait_def, wrappers, target_version)?
        .into_iter()
        .filter(|method| !method.wire.internal)
        .collect())
}

fn included_wire_methods<'a>(
    trait_def: &'a TraitDef,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<Vec<&'a MethodDef>> {
    trait_def
        .methods
        .iter()
        .filter_map(|method| {
            match method_is_included(trait_def, method, wrappers, target_version) {
                Ok(true) => Some(Ok(method)),
                Ok(false) => None,
                Err(err) => Some(Err(err)),
            }
        })
        .collect()
}

/// Lowered method payload: the TS param list and the inner value expression
/// the generated client wraps in the method's envelope. The public method
/// signature stays ergonomic (inner version types), while the generated
/// client owns versioned wrapper encoding.
struct PayloadEmission {
    /// Comma-separated `name: Type` entries used as the body of the user-facing
    /// object argument type. Empty when the method takes no input.
    param_list: String,
    /// Local names destructured in the method signature and referenced by
    /// `value_expr`.
    param_names: Vec<String>,
    inner_type_ts: String,
    value_expr: String,
}

fn emit_payload(
    params: &[ParamDef],
    wrappers: &BTreeMap<String, VersionedWrapper>,
    wire_version: Option<u32>,
) -> Result<PayloadEmission> {
    // The unified contract always takes a single versioned-wrapper arg. On the
    // TS side callers pass the inner value directly, but we re-wrap before
    // handing bytes to the transport. The host-side dispatcher decodes the
    // full wrapper (variant byte included) from the wire payload.
    if params.len() == 1
        && let Some((wrapper_name, wrapper)) = versioned_wrapper_for(&params[0].type_ref, wrappers)
    {
        let version = wire_version.ok_or_else(|| {
            anyhow::anyhow!("versioned wrapper `{wrapper_name}` has no selected wire version")
        })?;
        let wrapper = wrapper.variants.get(&version).ok_or_else(|| {
            anyhow::anyhow!("versioned wrapper `{wrapper_name}` has no V{version} variant")
        })?;
        return match &wrapper.kind {
            VersionedKind::Unit => Ok(PayloadEmission {
                param_list: String::new(),
                param_names: Vec::new(),
                inner_type_ts: "undefined".to_string(),
                value_expr: "undefined".to_string(),
            }),
            VersionedKind::Tuple(inner) => {
                let inner_ts = ts_type_qualified(inner)?;
                Ok(PayloadEmission {
                    param_list: format!("request: {inner_ts}"),
                    param_names: vec!["request".to_string()],
                    inner_type_ts: inner_ts,
                    value_expr: "request".to_string(),
                })
            }
        };
    }

    if params.is_empty() {
        return Ok(PayloadEmission {
            param_list: String::new(),
            param_names: Vec::new(),
            inner_type_ts: "undefined".to_string(),
            value_expr: "undefined".to_string(),
        });
    }

    let inner_type_ts = payload_type(params)?;
    Ok(PayloadEmission {
        param_list: format!("request: {inner_type_ts}"),
        param_names: vec!["request".to_string()],
        inner_type_ts: inner_type_ts.clone(),
        value_expr: "request".to_string(),
    })
}

/// Response shape after the versioned wrapper is stripped. TS callers see the
/// bare inner type; decoding through the wrapper's own codec directly (its
/// `V<N>` tag is the version) and unwrapping `.value` produces it with no
/// further translation.
#[derive(Clone)]
struct ResponseEmission {
    inner_type_ts: String,
}

fn emit_response(
    ty: &TypeRef,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    wire_version: Option<u32>,
) -> Result<ResponseEmission> {
    if let Some((wrapper_name, wrapper)) = versioned_wrapper_for(ty, wrappers) {
        let version = wire_version.ok_or_else(|| {
            anyhow::anyhow!("versioned wrapper `{wrapper_name}` has no selected wire version")
        })?;
        let wrapper = wrapper.variants.get(&version).ok_or_else(|| {
            anyhow::anyhow!("versioned wrapper `{wrapper_name}` has no V{version} variant")
        })?;
        return match &wrapper.kind {
            VersionedKind::Unit => Ok(ResponseEmission {
                inner_type_ts: "undefined".to_string(),
            }),
            VersionedKind::Tuple(inner) => Ok(ResponseEmission {
                inner_type_ts: ts_type_qualified(inner)?,
            }),
        };
    }

    Ok(ResponseEmission {
        inner_type_ts: ts_type_qualified(ty)?,
    })
}

fn emit_error_response(
    ty: &TypeRef,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    wire_version: Option<u32>,
) -> Result<ResponseEmission> {
    let Some(error_wrapper_ty) = call_error_inner(ty) else {
        return emit_response(ty, wrappers, wire_version);
    };

    if let Some((wrapper_name, _wrapper)) = versioned_wrapper_for(error_wrapper_ty, wrappers) {
        let versioned_name = versioned_wrapper_ts_name(wrapper_name);
        let inner_type_ts = format!("S.CallErrorValue<T.{versioned_name}>");
        return Ok(ResponseEmission { inner_type_ts });
    }

    let inner_type_ts = format!(
        "S.CallErrorValue<{}>",
        ts_type_qualified_preserve(error_wrapper_ty)?
    );
    Ok(ResponseEmission { inner_type_ts })
}

/// The codec expression that decodes this leg's own wire payload directly: the
/// recognized wrapper's `Versioned{Name}` codec, whose `V<N>` tag is the wire's
/// only version signal. Every method's request, response and item is one of
/// these, an empty payload included; anything else has no describable codec.
fn leg_codec_expr(ty: &TypeRef, wrappers: &BTreeMap<String, VersionedWrapper>) -> Result<String> {
    match versioned_wrapper_for(ty, wrappers) {
        Some((wrapper_name, _)) => Ok(format!("T.{}", versioned_wrapper_ts_name(wrapper_name))),
        None => bail!(
            "type `{}` is not a recognized versioned wrapper, \
             so no wire codec can be derived for it",
            ts_type_name_hint(ty)
        ),
    }
}

/// The codec expression that decodes an error leg's wire payload: the
/// recognized domain wrapper's `Versioned{Name}` codec composed with
/// `S.CallError`, or a plain `S.CallError({codec})` over the bare error type
/// when it isn't a recognized wrapper (framework-only errors have no domain
/// payload at all, so this path is never reached for those).
fn leg_error_codec_expr(
    ty: &TypeRef,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    ctx: &CodecContext,
) -> Result<String> {
    // Every real method's error type is `CallError<X>`; a bare (non-CallError)
    // type is decoded as an ordinary leg with no framework-error envelope.
    let Some(error_wrapper_ty) = call_error_inner(ty) else {
        return leg_codec_expr(ty, wrappers);
    };
    if let Some((wrapper_name, _)) = versioned_wrapper_for(error_wrapper_ty, wrappers) {
        return Ok(format!(
            "S.CallError(T.{})",
            versioned_wrapper_ts_name(wrapper_name)
        ));
    }
    Ok(format!(
        "S.CallError({})",
        codec_expr_mode(error_wrapper_ty, true, ctx, NameMode::Public)?
    ))
}

/// A human-readable name for an error message when a type ref that was
/// expected to be a recognized versioned wrapper is not one.
fn ts_type_name_hint(ty: &TypeRef) -> &str {
    match ty {
        TypeRef::Named { name, .. } => name,
        _ => "<anonymous>",
    }
}

fn versioned_kind_codec_expr_mode(
    kind: &VersionedKind,
    qualified: bool,
    ctx: &CodecContext,
    mode: NameMode<'_>,
) -> Result<String> {
    match kind {
        VersionedKind::Unit => Ok("S._void".to_string()),
        VersionedKind::Tuple(inner) => codec_expr_mode(inner, qualified, ctx, mode),
    }
}

/// Builds a `S.indexedTaggedUnion({...})` expression for versioned wrapper
/// variants. Each `V<N>` arm uses wire discriminant `N - 1`, matching the
/// Rust `#[codec(index = N - 1)]` annotation.
fn indexed_versioned_codec_expr(
    variants: impl IntoIterator<Item = (u32, String)>,
) -> Result<String> {
    let mut entries = Vec::new();
    for (version, codec) in variants {
        let index = version
            .checked_sub(1)
            .ok_or_else(|| anyhow::anyhow!("versioned wrapper uses invalid V0 variant"))?;
        entries.push(format!("V{version}: [{index}, {codec}] as const"));
    }
    Ok(format!(
        "S.indexedTaggedUnion({{{entries}}})",
        entries = entries.join(", ")
    ))
}

/// The request wrapper name for a method's single param, if its param shape
/// is a recognized versioned wrapper (payloadless and multi/raw-param
/// methods have no such wrapper to name).
fn request_wrapper_name<'a>(
    method: &'a MethodDef,
    wrappers: &'a BTreeMap<String, VersionedWrapper>,
) -> Option<&'a str> {
    if method.params.len() != 1 {
        return None;
    }
    versioned_wrapper_for(&method.params[0].type_ref, wrappers).map(|(name, _)| name)
}

fn emit_method(
    out: &mut String,
    _api: &ApiDefinition,
    trait_def: &TraitDef,
    method: &MethodDef,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<()> {
    let ctx = CodecContext::default();
    let ts_method_name = to_camel_case(&strip_prefix(&method.name));
    let wire_const = wire_const_name(&trait_def.name, &method.name);
    let wire_version = method_wire_version(method, wrappers, target_version)?;
    let payload = emit_payload(&method.params, wrappers, wire_version)?;
    write_jsdoc(out, "  ", method.docs.as_deref());

    if method.wire.host_initiated {
        return emit_host_initiated_method(out, method, &payload, wrappers, wire_version);
    }

    let version = wire_version.ok_or_else(|| {
        anyhow::anyhow!(
            "method `{}` has no request/item wrapper with a selected wire version",
            method.name
        )
    })?;
    // Every `Request` method's single param is a recognized wrapper; a
    // subscription may legitimately take none at all (its `Start` payload is
    // then empty bytes), see `emit_subscribe_method`.
    let request_name = request_wrapper_name(method, wrappers);
    // Declaring a parameter that is not a versioned wrapper is the one case
    // neither shape can emit: it has no wire payload. Checked here rather than
    // per arm, because `emit_payload` has already put the parameter in the
    // signature by this point, and the subscription arms would otherwise fall
    // through to an empty `Start` payload and silently drop it.
    if !method.params.is_empty() && request_name.is_none() {
        bail!(
            "method `{}`: request parameter is not a recognized versioned wrapper",
            method.name
        );
    }

    match (&method.kind, &method.return_type) {
        (MethodKind::Request, ReturnType::Result { ok, err }) => {
            let Some(request_name) = request_name else {
                bail!(
                    "method `{}`: request parameter is not a recognized versioned wrapper",
                    method.name
                );
            };
            let request_codec = format!("T.{}", versioned_wrapper_ts_name(request_name));
            let is_handshake = trait_def.name == "System" && method.name == "handshake";
            let response = emit_response(ok, wrappers, wire_version)?;
            let error = emit_error_response(err, wrappers, wire_version)?;
            let ok_is_wrapper = versioned_wrapper_for(ok, wrappers).is_some();
            let response_codec = leg_codec_expr(ok, wrappers)?;
            let error_codec = leg_error_codec_expr(err, wrappers, &ctx)?;

            // Every request method takes the cancellation signal last, so a
            // product can withdraw any call without the method opting in.
            let arg_decl = if is_handshake || payload.param_list.is_empty() {
                "options?: CallOptions".to_string()
            } else {
                format!("request: {}, options?: CallOptions", payload.inner_type_ts)
            };
            let request_expr = if is_handshake {
                "{ codecVersion: TRUAPI_CODEC_VERSION }".to_string()
            } else {
                payload.value_expr.clone()
            };

            writedoc!(
                out,
                "
                  {ts_method_name}({arg_decl}): ResultAsync<{ok_type}, {err_type}> {{
                    return this.#transport.request<{ok_type}, {err_type}>({{
                      ids: W.{wire_const},
                      payload: {request_codec}.enc({{ tag: \"V{version}\", value: {request_expr} }}),
                      signal: options?.signal,
                      decodeResponse: (payload) => {{
                        const result = S.Result({response_codec}, {error_codec}).dec(payload);
                ",
                ok_type = response.inner_type_ts,
                err_type = error.inner_type_ts
            )
            .unwrap();
            if ok_is_wrapper {
                writeln!(
                    out,
                    "        return result.success ? {{ success: true, value: result.value.value }} : result;"
                )
                .unwrap();
            } else {
                writeln!(out, "        return result;").unwrap();
            }
            writeln!(out, "      }},").unwrap();

            writedoc!(
                out,
                "
                    }});
                  }}
                "
            )
            .unwrap();
        }
        (MethodKind::Subscription, ReturnType::Subscription { item, interrupt }) => {
            let response = emit_response(item, wrappers, wire_version)?;
            let item_codec = leg_codec_expr(item, wrappers)?;
            let item_is_wrapper = versioned_wrapper_for(item, wrappers).is_some();
            let error = emit_error_response(interrupt, wrappers, wire_version)?;
            let error_codec = leg_error_codec_expr(interrupt, wrappers, &ctx)?;
            emit_subscribe_method(
                out,
                &ts_method_name,
                &wire_const,
                &payload,
                response.inner_type_ts.clone(),
                item_codec,
                item_is_wrapper,
                error,
                error_codec,
                version,
                request_name,
            )?;
        }
        (kind, return_type) => {
            bail!(
                "Generator internal mismatch for method `{}`: kind {:?} does not match return type {:?}",
                method.name,
                kind,
                return_type
            );
        }
    }

    Ok(())
}

fn host_registration_field(method: &MethodDef) -> String {
    format!(
        "#{}Registration",
        to_camel_case(&strip_prefix(&method.name))
    )
}

fn emit_host_initiated_types(
    method: &MethodDef,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<(PayloadEmission, ResponseEmission, ResponseEmission, u32)> {
    let wire_version = method_wire_version(method, wrappers, target_version)?.ok_or_else(|| {
        anyhow::anyhow!("host-initiated method `{}` is not versioned", method.name)
    })?;
    let payload = emit_payload(&method.params, wrappers, Some(wire_version))?;
    let ReturnType::Subscription { item, interrupt } = &method.return_type else {
        bail!(
            "host-initiated method `{}` must return Subscription<Item, Interrupt>",
            method.name
        );
    };
    let response = emit_response(item, wrappers, Some(wire_version))?;
    let error = emit_error_response(interrupt, wrappers, Some(wire_version))?;
    Ok((payload, response, error, wire_version))
}

fn emit_host_initiated_field(
    out: &mut String,
    method: &MethodDef,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<()> {
    let (payload, response, error, _) =
        emit_host_initiated_types(method, wrappers, target_version)?;
    writeln!(
        out,
        "  readonly {}: HostInitiatedSubscriptionRegistration<{}, {}, {}>;",
        host_registration_field(method),
        payload.inner_type_ts,
        response.inner_type_ts,
        error.inner_type_ts
    )
    .unwrap();
    Ok(())
}

fn emit_host_initiated_registration(
    out: &mut String,
    _api: &ApiDefinition,
    trait_def: &TraitDef,
    method: &MethodDef,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<()> {
    let (_, _, _, version) = emit_host_initiated_types(method, wrappers, target_version)?;
    let ctx = CodecContext::default();
    let wire_const = wire_const_name(&trait_def.name, &method.name);
    let ReturnType::Subscription {
        item: item_ty,
        interrupt,
    } = &method.return_type
    else {
        bail!(
            "host-initiated method `{}` must return Subscription<Item, Interrupt>",
            method.name
        );
    };
    let Some(request_name) = request_wrapper_name(method, wrappers) else {
        bail!(
            "host-initiated method `{}`: request parameter is not a recognized versioned wrapper",
            method.name
        );
    };
    let Some((item_name, _)) = versioned_wrapper_for(item_ty, wrappers) else {
        bail!(
            "host-initiated method `{}`: item type is not a recognized versioned wrapper",
            method.name
        );
    };
    let request_codec = format!("T.{}", versioned_wrapper_ts_name(request_name));
    let item_codec = format!("T.{}", versioned_wrapper_ts_name(item_name));
    let error_codec = leg_error_codec_expr(interrupt, wrappers, &ctx)?;
    writedoc!(
        out,
        "
            this.{field} = transport.registerHostInitiatedSubscription({{
              ids: W.{wire_const},
              decodeRequest: (payload) => {request_codec}.dec(payload).value,
              encodeItem: (item) => {item_codec}.enc({{ tag: \"V{version}\", value: item }}),
              encodeInterrupt: interruptEncoder({error_codec}),
              declinePayload: HOST_INITIATED_DECLINE_PAYLOAD,
              bufferCapacity: HOST_INITIATED_BUFFER_CAPACITY,
            }});
        ",
        field = host_registration_field(method),
    )
    .unwrap();
    Ok(())
}

fn emit_host_initiated_method(
    out: &mut String,
    method: &MethodDef,
    payload: &PayloadEmission,
    wrappers: &BTreeMap<String, VersionedWrapper>,
    wire_version: Option<u32>,
) -> Result<()> {
    let ReturnType::Subscription { item, interrupt } = &method.return_type else {
        bail!(
            "host-initiated method `{}` must return Subscription<Item, Interrupt>",
            method.name
        );
    };
    let response = emit_response(item, wrappers, wire_version)?;
    let error = emit_error_response(interrupt, wrappers, wire_version)?;
    let name = format!("on{}", strip_prefix(&method.name).to_case(Case::Pascal));
    writedoc!(
        out,
        "
          {name}(
            handler: HostInitiatedSubscriptionHandler<{request}, {item}, {reason}>,
          ): {{ unsubscribe(): void }} {{
            return this.{field}.setHandler(handler);
          }}
        ",
        request = payload.inner_type_ts,
        item = response.inner_type_ts,
        reason = error.inner_type_ts,
        field = host_registration_field(method),
    )
    .unwrap();
    Ok(())
}

/// Emits a subscribe method body that returns an Observable-compatible
/// object. `Start`'s payload is the request wrapper's own encoding, or empty
/// bytes when the method takes no request at all. The `Interrupt` payload is
/// `Result<(), CallError<Err>>`, so every generated method decodes `Ok(())`
/// as a clean completion and `Err(reason)` as `error`.
#[allow(clippy::too_many_arguments)]
fn emit_subscribe_method(
    out: &mut String,
    ts_method_name: &str,
    wire_const: &str,
    payload: &PayloadEmission,
    item_type_ts: String,
    item_codec: String,
    item_is_wrapper: bool,
    err: ResponseEmission,
    error_codec: String,
    version: u32,
    request_name: Option<&str>,
) -> Result<()> {
    let observable_args = format!("{item_type_ts}, {}", err.inner_type_ts);
    let signature = if payload.param_list.is_empty() {
        format!("  {ts_method_name}(): ObservableLike<{observable_args}> {{")
    } else {
        format!(
            "  {}({{ {} }}: {{ {} }}): ObservableLike<{}> {{",
            ts_method_name,
            payload.param_names.join(", "),
            payload.param_list,
            observable_args
        )
    };

    let start_payload = match request_name {
        Some(name) => format!(
            "T.{}.enc({{ tag: \"V{version}\", value: {} }})",
            versioned_wrapper_ts_name(name),
            payload.value_expr
        ),
        None => "new Uint8Array()".to_string(),
    };

    writedoc!(
        out,
        "
        {signature}
            return createObservable<{observable_args}>({{
              transport: this.#transport,
              ids: W.{wire_const},
              payload: {start_payload},
        "
    )
    .unwrap();
    if item_is_wrapper {
        writedoc!(
            out,
            "
              decodeItem: (payload) => {item_codec}.dec(payload).value,
            "
        )
        .unwrap();
    } else {
        writedoc!(
            out,
            "
              decodeItem: (payload) => {item_codec}.dec(payload),
            "
        )
        .unwrap();
    }
    writedoc!(
        out,
        "
              decodeInterrupt: interruptDecoder({error_codec}),
            }});
          }}
        "
    )
    .unwrap();

    Ok(())
}

fn emitted_type_name(
    ty: &TypeDef,
    emit_versions: &BTreeMap<String, BTreeSet<u32>>,
    aliases: &BTreeMap<String, String>,
) -> String {
    if should_rename_wire_wrapper(ty, emit_versions, aliases) {
        versioned_wrapper_ts_name(&ty.name)
    } else {
        aliases.get(&ty.name).unwrap_or(&ty.name).clone()
    }
}

fn write_type_definition(
    out: &mut String,
    ty: &TypeDef,
    emit_versions: &BTreeMap<String, BTreeSet<u32>>,
    aliases: &BTreeMap<String, String>,
) -> Result<()> {
    let generated_names = NameMode::Generated { aliases };
    let generic_decl = generic_param_declaration(&ty.generic_params);
    let emitted_name = emitted_type_name(ty, emit_versions, aliases);

    write_jsdoc(out, "", ty.docs.as_deref());
    match &ty.kind {
        TypeDefKind::Alias(type_ref) => {
            writeln!(
                out,
                "export type {emitted_name}{generic_decl} = {};",
                ts_type_with_named(type_ref, false, generated_names)?
            )
            .unwrap();
        }
        TypeDefKind::Struct(fields) => {
            writeln!(out, "export interface {emitted_name}{generic_decl} {{").unwrap();
            for field in fields {
                let (ts_name, optional) = ts_field_name(&field.name, &field.type_ref);
                write_jsdoc(out, "  ", field.docs.as_deref());
                if optional {
                    writeln!(
                        out,
                        "  {ts_name}?: {};",
                        ts_inner_option_with_named(&field.type_ref, false, generated_names)?
                    )
                    .unwrap();
                } else {
                    writeln!(
                        out,
                        "  {ts_name}: {};",
                        ts_type_with_named(&field.type_ref, false, generated_names)?
                    )
                    .unwrap();
                }
            }
            writeln!(out, "}}").unwrap();
        }
        TypeDefKind::TupleStruct(fields) => {
            writeln!(
                out,
                "export type {emitted_name}{generic_decl} = {};",
                unnamed_fields_type_mode(fields, false, generated_names)?
            )
            .unwrap();
        }
        TypeDefKind::Enum(variants) => {
            if is_unit_only_enum(ty) {
                writeln!(
                    out,
                    "export type {emitted_name}{generic_decl} = {};",
                    unit_enum_union_type(variants)?
                )
                .unwrap();
            } else {
                // For versioned wrappers, only emit the variant(s) the client
                // actually wire-encodes. The wire byte index is preserved by the
                // codec definition (`indexed_versioned_codec_expr`).
                let selected = emit_versions.get(&ty.name);
                writeln!(out, "export type {emitted_name}{generic_decl} =").unwrap();
                for variant in variants {
                    if let Some(versions) = selected {
                        let Some(version) = version_number(&variant.name) else {
                            continue;
                        };
                        if !versions.contains(&version) {
                            continue;
                        }
                    }
                    write_jsdoc(out, "  ", variant.docs.as_deref());
                    writeln!(
                        out,
                        "  | {}",
                        enum_variant_ts_type_mode(variant, generated_names)?
                    )
                    .unwrap();
                }
                writeln!(out, ";").unwrap();
            }
        }
    }

    Ok(())
}

fn write_codec_definition(
    out: &mut String,
    ty: &TypeDef,
    emit_versions: &BTreeMap<String, BTreeSet<u32>>,
    aliases: &BTreeMap<String, String>,
) -> Result<()> {
    let generated_names = NameMode::Generated { aliases };
    if ty.generic_params.is_empty() {
        let ctx = codec_context(&[]);
        if let Some(wrapper) = detect_versioned_wrapper(ty) {
            let selected = emit_versions.get(&ty.name);
            let emitted_name = if should_rename_wire_wrapper(ty, emit_versions, aliases) {
                versioned_wrapper_ts_name(&ty.name)
            } else if let Some(alias) = aliases.get(&ty.name) {
                alias.clone()
            } else {
                ty.name.clone()
            };
            let codec_expr = indexed_versioned_codec_expr(
                wrapper
                    .variants
                    .values()
                    .filter(|variant| {
                        selected.is_none_or(|versions| versions.contains(&variant.version))
                    })
                    .map(|variant| {
                        Ok((
                            variant.version,
                            versioned_kind_codec_expr_mode(
                                &variant.kind,
                                false,
                                &ctx,
                                generated_names,
                            )?,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?,
            )?;
            let type_name = top_level_type_name(&emitted_name, &ty.generic_params);
            writeln!(
                out,
                "export const {emitted_name}: S.Codec<{type_name}> = S.lazy((): S.Codec<{type_name}> => {codec_expr});",
            )
            .unwrap();
            return Ok(());
        }
        let emitted_name = aliases
            .get(&ty.name)
            .map(String::as_str)
            .unwrap_or(&ty.name);
        let type_name = top_level_type_name(emitted_name, &ty.generic_params);
        let codec_expr =
            type_codec_expr_mode_qualified(ty, &type_name, &ctx, generated_names, false)?;
        writeln!(
            out,
            "export const {emitted_name}: S.Codec<{type_name}> = S.lazy((): S.Codec<{type_name}> => {codec_expr});",
        )
        .unwrap();
        return Ok(());
    }

    let generic_decl = generic_param_declaration(&ty.generic_params);
    let codec_params = ty
        .generic_params
        .iter()
        .map(|param| format!("{}: S.Codec<{}>", codec_param_name(param), param))
        .collect::<Vec<_>>()
        .join(", ");
    let ctx = codec_context(&ty.generic_params);
    let type_name = top_level_type_name(
        aliases
            .get(&ty.name)
            .map(String::as_str)
            .unwrap_or(&ty.name),
        &ty.generic_params,
    );

    if ty.name == "Component" {
        writedoc!(
            out,
            "
            /** Builds a codec for renderer components parameterized by the codec of their
             * `props` payload. */
            "
        )
        .unwrap();
    }
    let function_name = aliases
        .get(&ty.name)
        .map(String::as_str)
        .unwrap_or(&ty.name);
    let codec_body = type_codec_expr_mode_qualified(ty, &type_name, &ctx, generated_names, false)?;
    writedoc!(
        out,
        "
        export function {function_name}{generic_decl}({codec_params}): S.Codec<{type_name}> {{
          return S.lazy((): S.Codec<{type_name}> => {codec_body});
        }}
        ",
    )
    .unwrap();

    Ok(())
}

fn should_rename_wire_wrapper(
    ty: &TypeDef,
    emit_versions: &BTreeMap<String, BTreeSet<u32>>,
    aliases: &BTreeMap<String, String>,
) -> bool {
    detect_versioned_wrapper(ty).is_some()
        && (emit_versions.contains_key(&ty.name) || aliases.values().any(|alias| alias == &ty.name))
}

fn type_codec_expr_mode_qualified(
    ty: &TypeDef,
    type_name: &str,
    ctx: &CodecContext,
    mode: NameMode<'_>,
    qualified: bool,
) -> Result<String> {
    match &ty.kind {
        TypeDefKind::Alias(type_ref) => codec_expr_mode(type_ref, qualified, ctx, mode),
        TypeDefKind::Struct(fields) => {
            struct_codec_expr_mode(fields, type_name, qualified, ctx, mode)
        }
        TypeDefKind::TupleStruct(fields) => {
            unnamed_fields_codec_expr_mode(fields, qualified, ctx, mode)
        }
        TypeDefKind::Enum(variants) => {
            if is_unit_only_enum(ty) {
                unit_enum_codec_expr(variants)
            } else {
                let variants = variants
                    .iter()
                    .map(|variant| {
                        Ok(format!(
                            "{}: {}",
                            variant.name,
                            variant_codec_expr_mode(&variant.fields, qualified, ctx, mode)?
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("S.TaggedUnion({{{variants}}})"))
            }
        }
    }
}

fn is_unit_only_enum(ty: &TypeDef) -> bool {
    detect_versioned_wrapper(ty).is_none()
        && matches!(
            &ty.kind,
            TypeDefKind::Enum(variants)
                if !variants.is_empty()
                    && variants
                        .iter()
                        .all(|variant| matches!(variant.fields, VariantFields::Unit))
        )
}

fn unit_enum_union_type(variants: &[VariantDef]) -> Result<String> {
    Ok(variants
        .iter()
        .map(|variant| ts_string_literal(&variant.name))
        .collect::<Vec<_>>()
        .join(" | "))
}

fn unit_enum_codec_expr(variants: &[VariantDef]) -> Result<String> {
    Ok(format!(
        "S.Status({})",
        variants
            .iter()
            .map(|variant| ts_string_literal(&variant.name))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn unit_enum_summary(variants: &[VariantDef]) -> String {
    format!(
        "Enum values: {}",
        variants
            .iter()
            .map(|variant| variant.name.as_str())
            .collect::<Vec<_>>()
            .join(" / ")
    )
}

fn variant_value_type_mode(fields: &VariantFields, mode: NameMode<'_>) -> Result<String> {
    let qualified = false;
    match fields {
        VariantFields::Unit => Ok("undefined".to_string()),
        VariantFields::Unnamed(types) => unnamed_fields_type_mode(types, qualified, mode),
        VariantFields::Named(fields) => inline_object_type_mode(fields, qualified, mode),
    }
}

/// Renders the public TS type for a single enum variant. Unit variants mark
/// `value` optional (`value?: undefined`) so consumers can write
/// `{ tag: "X" }` while the codec round-trip (`{ tag, value: undefined }`)
/// still type-checks.
fn enum_variant_ts_type(variant: &VariantDef) -> Result<String> {
    enum_variant_ts_type_mode(variant, NameMode::Public)
}

fn enum_variant_ts_type_mode(variant: &VariantDef, mode: NameMode<'_>) -> Result<String> {
    Ok(match &variant.fields {
        VariantFields::Unit => format!("{{ tag: \"{}\"; value?: undefined }}", variant.name),
        fields => format!(
            "{{ tag: \"{}\"; value: {} }}",
            variant.name,
            variant_value_type_mode(fields, mode)?
        ),
    })
}

fn variant_codec_expr_mode(
    fields: &VariantFields,
    qualified: bool,
    ctx: &CodecContext,
    mode: NameMode<'_>,
) -> Result<String> {
    match fields {
        VariantFields::Unit => Ok("S._void".to_string()),
        VariantFields::Unnamed(types) => {
            unnamed_fields_codec_expr_mode(types, qualified, ctx, mode)
        }
        VariantFields::Named(fields) => struct_codec_expr_mode(
            fields,
            &inline_object_type_mode(fields, qualified, mode)?,
            qualified,
            ctx,
            mode,
        ),
    }
}

fn unnamed_fields_type(types: &[TypeRef]) -> Result<String> {
    unnamed_fields_type_mode(types, false, NameMode::Public)
}

fn unnamed_fields_type_mode(
    types: &[TypeRef],
    qualified: bool,
    mode: NameMode<'_>,
) -> Result<String> {
    if types.is_empty() {
        Ok("undefined".to_string())
    } else if types.len() == 1 {
        ts_type_with_named(&types[0], qualified, mode)
    } else {
        Ok(format!(
            "[{}]",
            types
                .iter()
                .map(|ty| ts_type_with_named(ty, qualified, mode))
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        ))
    }
}

fn unnamed_fields_codec_expr_mode(
    types: &[TypeRef],
    qualified: bool,
    ctx: &CodecContext,
    mode: NameMode<'_>,
) -> Result<String> {
    if types.is_empty() {
        Ok("S._void".to_string())
    } else if types.len() == 1 {
        codec_expr_mode(&types[0], qualified, ctx, mode)
    } else {
        let codecs = types
            .iter()
            .map(|ty| codec_expr_mode(ty, qualified, ctx, mode))
            .collect::<Result<Vec<_>>>()?
            .join(", ");
        Ok(format!("S.Tuple({codecs})"))
    }
}

fn struct_codec_expr_mode(
    fields: &[FieldDef],
    type_name: &str,
    qualified: bool,
    ctx: &CodecContext,
    mode: NameMode<'_>,
) -> Result<String> {
    let field_specs = fields
        .iter()
        .map(|field| {
            let (name, _optional) = ts_field_name(&field.name, &field.type_ref);
            Ok(format!(
                "{}: {}",
                name,
                codec_expr_mode(&field.type_ref, qualified, ctx, mode)?
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    Ok(format!(
        "S.Struct({{{field_specs}}}) as S.Codec<{type_name}>"
    ))
}

fn inline_object_type_mode(
    fields: &[FieldDef],
    qualified: bool,
    mode: NameMode<'_>,
) -> Result<String> {
    Ok(format!(
        "{{ {} }}",
        fields
            .iter()
            .map(|field| {
                let (name, optional) = ts_field_name(&field.name, &field.type_ref);
                if optional {
                    Ok(format!(
                        "{}?: {}",
                        name,
                        ts_inner_option_with_named(&field.type_ref, qualified, mode)?
                    ))
                } else {
                    Ok(format!(
                        "{}: {}",
                        name,
                        ts_type_with_named(&field.type_ref, qualified, mode)?
                    ))
                }
            })
            .collect::<Result<Vec<_>>>()?
            .join("; ")
    ))
}

fn codec_expr_mode(
    ty: &TypeRef,
    qualified: bool,
    ctx: &CodecContext,
    mode: NameMode<'_>,
) -> Result<String> {
    match ty {
        TypeRef::Primitive(name) => match name.as_str() {
            "bool" => Ok("S.bool".to_string()),
            "u8" => Ok("S.u8".to_string()),
            "u16" => Ok("S.u16".to_string()),
            "u32" => Ok("S.u32".to_string()),
            "u64" => Ok("S.u64".to_string()),
            "u128" => Ok("S.u128".to_string()),
            name if name.starts_with("compact") => Ok("S.compact".to_string()),
            "optionBool" => Ok("S.OptionBool".to_string()),
            "i8" => Ok("S.i8".to_string()),
            "i16" => Ok("S.i16".to_string()),
            "i32" => Ok("S.i32".to_string()),
            "i64" => Ok("S.i64".to_string()),
            "i128" => Ok("S.i128".to_string()),
            "str" => Ok("S.str".to_string()),
            _ => bail!("Unsupported primitive type `{name}` in TypeScript codec generation"),
        },
        TypeRef::Named { name, args } => {
            if name == "CallError" && args.len() == 1 {
                return Ok(format!(
                    "S.CallError({})",
                    codec_expr_mode(&args[0], qualified, ctx, mode)?
                ));
            }
            let resolved = resolve_named(name, mode);
            let target = if qualified {
                qualify_named(&resolved, mode)
            } else {
                resolved
            };

            if args.is_empty() {
                Ok(target)
            } else {
                let codecs = args
                    .iter()
                    .map(|arg| codec_expr_mode(arg, qualified, ctx, mode))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("{target}({codecs})"))
            }
        }
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Primitive(name) if name == "u8" => Ok("S.Hex()".to_string()),
            _ => Ok(format!(
                "S.Vector({})",
                codec_expr_mode(inner, qualified, ctx, mode)?
            )),
        },
        TypeRef::Option(inner) => Ok(format!(
            "S.Option({})",
            codec_expr_mode(inner, qualified, ctx, mode)?
        )),
        TypeRef::Tuple(items) => {
            if items.is_empty() {
                Ok("S._void".to_string())
            } else {
                let codecs = items
                    .iter()
                    .map(|item| codec_expr_mode(item, qualified, ctx, mode))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("S.Tuple({codecs})"))
            }
        }
        TypeRef::Array(inner, len) => match inner.as_ref() {
            TypeRef::Primitive(name) if name == "u8" => Ok(format!("S.Hex({len})")),
            _ => Ok(format!(
                "S.Vector({}, {})",
                codec_expr_mode(inner, qualified, ctx, mode)?,
                len
            )),
        },
        TypeRef::Generic(name) => ctx
            .generic_codecs
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Missing codec for generic parameter `{name}`")),
        TypeRef::Unit => Ok("S._void".to_string()),
    }
}

fn ts_type(ty: &TypeRef) -> Result<String> {
    ts_type_with_named(ty, false, NameMode::Public)
}

fn ts_type_with_named(ty: &TypeRef, qualified: bool, mode: NameMode<'_>) -> Result<String> {
    match ty {
        TypeRef::Primitive(name) => match name.as_str() {
            "bool" => Ok("boolean".to_string()),
            "u8" | "u16" | "u32" | "i8" | "i16" | "i32" | "f32" | "f64" => Ok("number".to_string()),
            "u64" | "u128" | "i64" | "i128" => Ok("bigint".to_string()),
            name if name.starts_with("compact") => Ok("number | bigint".to_string()),
            "optionBool" => Ok("boolean | undefined".to_string()),
            "str" => Ok("string".to_string()),
            _ => bail!("Unsupported primitive type `{name}` in TypeScript type generation"),
        },
        TypeRef::Named { name, args } => {
            if name == "CallError" && args.len() == 1 {
                return Ok(format!(
                    "S.CallErrorValue<{}>",
                    ts_type_with_named(&args[0], qualified, mode)?
                ));
            }
            let resolved = resolve_named(name, mode);
            let target = if qualified {
                qualify_named(&resolved, mode)
            } else {
                resolved
            };

            if args.is_empty() {
                Ok(target)
            } else {
                let args = args
                    .iter()
                    .map(|arg| ts_type_with_named(arg, qualified, mode))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("{target}<{args}>"))
            }
        }
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Primitive(name) if name == "u8" => Ok(hex_string_ts_name(qualified)),
            _ => Ok(format!(
                "Array<{}>",
                ts_type_with_named(inner, qualified, mode)?
            )),
        },
        TypeRef::Option(inner) => Ok(format!(
            "{} | undefined",
            ts_type_with_named(inner, qualified, mode)?
        )),
        TypeRef::Tuple(items) => {
            if items.is_empty() {
                Ok("undefined".to_string())
            } else {
                Ok(format!(
                    "[{}]",
                    items
                        .iter()
                        .map(|item| ts_type_with_named(item, qualified, mode))
                        .collect::<Result<Vec<_>>>()?
                        .join(", ")
                ))
            }
        }
        TypeRef::Array(inner, _len) => match inner.as_ref() {
            TypeRef::Primitive(name) if name == "u8" => Ok(hex_string_ts_name(qualified)),
            _ => Ok(format!(
                "Array<{}>",
                ts_type_with_named(inner, qualified, mode)?
            )),
        },
        TypeRef::Generic(name) => Ok(name.clone()),
        TypeRef::Unit => Ok("undefined".to_string()),
    }
}

/// Always emit the user-facing `HexString` name (no codec-namespace prefix).
/// Generated `types.ts` imports it directly from `scale.js`.
fn hex_string_ts_name(_qualified: bool) -> String {
    "HexString".to_string()
}

fn ts_inner_option(ty: &TypeRef) -> Result<String> {
    ts_inner_option_with_named(ty, false, NameMode::Public)
}

fn ts_inner_option_with_named(ty: &TypeRef, qualified: bool, mode: NameMode<'_>) -> Result<String> {
    match ty {
        TypeRef::Option(inner) => ts_type_with_named(inner, qualified, mode),
        TypeRef::Primitive(name) if name == "optionBool" => Ok("boolean".to_string()),
        other => ts_type_with_named(other, qualified, mode),
    }
}

fn ts_type_qualified(ty: &TypeRef) -> Result<String> {
    ts_type_with_named(ty, true, NameMode::Public)
}

fn ts_type_qualified_preserve(ty: &TypeRef) -> Result<String> {
    ts_type_with_named(ty, true, NameMode::PreserveQualified)
}

fn ts_field_name(name: &str, ty: &TypeRef) -> (String, bool) {
    let camel = to_camel_case(name);
    let optional = matches!(ty, TypeRef::Option(_))
        || matches!(ty, TypeRef::Primitive(name) if name == "optionBool");
    (camel, optional)
}

fn payload_type(params: &[ParamDef]) -> Result<String> {
    payload_type_mode(params, NameMode::Public)
}

fn payload_type_mode(params: &[ParamDef], mode: NameMode<'_>) -> Result<String> {
    match params.len() {
        0 => Ok("undefined".to_string()),
        1 => ts_type_with_named(&params[0].type_ref, true, mode),
        _ => Ok(format!(
            "[{}]",
            params
                .iter()
                .map(|param| ts_type_with_named(&param.type_ref, true, mode))
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        )),
    }
}

fn generic_param_declaration(params: &[String]) -> String {
    if params.is_empty() {
        String::new()
    } else {
        format!("<{}>", params.join(", "))
    }
}

fn top_level_type_name(name: &str, generic_params: &[String]) -> String {
    if generic_params.is_empty() {
        name.to_string()
    } else {
        format!("{}<{}>", name, generic_params.join(", "))
    }
}

fn codec_context(generic_params: &[String]) -> CodecContext {
    let generic_codecs = generic_params
        .iter()
        .map(|param| (param.clone(), codec_param_name(param)))
        .collect();
    CodecContext { generic_codecs }
}

fn codec_param_name(name: &str) -> String {
    format!("{}Codec", to_camel_case(name))
}

fn service_display_name(trait_def: &TraitDef) -> String {
    humanize_service_name(&trait_def.name)
}

fn humanize_service_name(name: &str) -> String {
    let display_name = name
        .to_case(Case::Title)
        .split_whitespace()
        .map(|word| match word {
            "Api" => "API".to_string(),
            "Id" => "ID".to_string(),
            "Json" => "JSON".to_string(),
            "Rpc" => "RPC".to_string(),
            "Url" => "URL".to_string(),
            _ => word.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ");

    if display_name == "JSON RPC" {
        "JSON-RPC".to_string()
    } else {
        display_name
    }
}

fn strip_prefix(name: &str) -> String {
    for prefix in ["host_", "remote_", "product_"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    name.to_string()
}

fn to_camel_case(s: &str) -> String {
    s.to_case(Case::Camel)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire_attrs(id: Option<u8>) -> WireAttrs {
        WireAttrs {
            id,
            ..WireAttrs::default()
        }
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
                    .map(|(name, type_ref)| FieldDef {
                        name: name.to_string(),
                        type_ref,
                        docs: None,
                    })
                    .collect(),
            ),
            docs: None,
        };
        let method = MethodDef {
            name: "do_thing".to_string(),
            kind: MethodKind::Request,
            params: vec![ParamDef {
                name: "request".to_string(),
                type_ref: TypeRef::Named {
                    name: "Payload".to_string(),
                    args: Vec::new(),
                },
            }],
            return_type: ReturnType::Result {
                ok: TypeRef::Unit,
                err: TypeRef::Unit,
            },
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
        for name in [
            "HiddenRequest",
            "PublicRequest",
            "SharedResponse",
            "SharedError",
        ] {
            let inner = format!("V01{name}");
            api.types
                .push(versioned_tuple_wrapper_variants(name, &[(1, &inner)]));
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
        assert_eq!(
            generate_index(),
            "export * from './types.js';\nexport * from './client.js';\n"
        );
    }

    #[test]
    fn internal_bindings_keep_typed_calls_separate_and_immutable() {
        let api = internal_api_fixture();
        let directory = tempfile::tempdir().unwrap();
        generate(&api, directory.path().to_str().unwrap(), 1, 2).unwrap();
        let internal = fs::read_to_string(directory.path().join("internal-client.ts")).unwrap();
        assert!(internal.contains("import * as T from './internal.js'"));
        assert!(
            internal
                .contains("createInternalClient(transport: TrUApiTransport): InternalTrUApiClient")
        );
        assert!(
            internal.contains("return this.#transport.request<T.SharedResponse, T.SharedError>"),
            "{internal}"
        );
        assert!(internal.contains("ids: W.THING_DO_THING"));
        assert!(internal.contains("T.VersionedHiddenRequest.enc"));
        assert!(
            internal.contains(
                "S.Result(T.VersionedSharedResponse, T.VersionedSharedError).dec(payload)"
            )
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
            source.contains(
                "S.Result(T.VersionedSharedResponse, T.VersionedSharedError).dec(payload)"
            )
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
        method.return_type = ReturnType::Subscription {
            item: ok,
            interrupt: err,
        };
        method.wire.host_initiated = true;
        let mut api = api(vec![method]);
        api.public_trait_order = vec!["Example".to_string()];
        for name in ["RenderRequest", "RenderItem", "RenderError"] {
            let inner = format!("V01{name}");
            api.types
                .push(versioned_tuple_wrapper_variants(name, &[(1, &inner)]));
            api.types.push(empty_struct(&inner));
        }
        let source = generate_client(&api, 1, 2).unwrap();
        assert!(
            source.contains("readonly #renderRegistration: HostInitiatedSubscriptionRegistration"),
            "{source}"
        );
        assert!(
            source
                .contains("this.#renderRegistration = transport.registerHostInitiatedSubscription")
        );
        assert!(source.contains("return this.#renderRegistration.setHandler(handler)"));
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
        let wrapper = api
            .types
            .iter_mut()
            .find(|ty| ty.name == "HiddenRequest")
            .unwrap();
        *wrapper = versioned_tuple_wrapper_variants(
            "HiddenRequest",
            &[(1, "Shared"), (2, "FutureShared")],
        );
        let TypeDefKind::Enum(variants) = &mut wrapper.kind else {
            unreachable!()
        };
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

        assert_ne!(
            wire_schema_hash(&before, 1, 1).unwrap(),
            wire_schema_hash(&after, 1, 1).unwrap(),
        );
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

        assert_ne!(
            wire_schema_hash(&before, 1, 1).unwrap(),
            wire_schema_hash(&after, 1, 1).unwrap(),
        );
    }

    #[test]
    fn schema_hash_is_stable_for_an_unchanged_contract() {
        // The fingerprint must not be noisy: an identical contract hashes
        // identically, or every host would look drifted.
        let api =
            api_with_payload_fields(vec![("ring_index", TypeRef::Primitive("u32".to_string()))]);

        assert_eq!(
            wire_schema_hash(&api, 1, 1).unwrap(),
            wire_schema_hash(&api, 1, 1).unwrap(),
        );
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
            &TypeRef::Named {
                name: "Node".to_string(),
                args: Vec::new(),
            },
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
            api_with_payload_fields(vec![(
                "size",
                TypeRef::Primitive(format!("compact<{width}>")),
            )])
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
                    type_ref: TypeRef::Named {
                        name: "Choice".to_string(),
                        args: Vec::new(),
                    },
                }],
                return_type: ReturnType::Result {
                    ok: TypeRef::Unit,
                    err: TypeRef::Unit,
                },
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
                return_type: ReturnType::Result {
                    ok: TypeRef::Unit,
                    err: TypeRef::Unit,
                },
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
                type_ref: TypeRef::Named {
                    name: "Missing".to_string(),
                    args: Vec::new(),
                },
            }],
            return_type: ReturnType::Result {
                ok: TypeRef::Unit,
                err: TypeRef::Unit,
            },
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

        let err = wire_schema_hash(&api, 1, 1)
            .expect_err("an unresolvable payload type must fail codegen");
        assert!(
            format!("{err}").contains("Missing"),
            "the error must name the offending type: {err}"
        );
    }

    #[test]
    fn a_resolvable_payload_hashes_without_complaint() {
        // The negative control: the guard must not fire on an ordinary payload,
        // or every codegen run would fail.
        let api =
            api_with_payload_fields(vec![("ring_index", TypeRef::Primitive("u32".to_string()))]);

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
            return_type: ReturnType::Result {
                ok: TypeRef::Unit,
                err: TypeRef::Unit,
            },
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
                    args: vec![TypeRef::Named {
                        name: "GenericError".to_string(),
                        args: vec![],
                    }],
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
        TypeRef::Named {
            name: name.to_string(),
            args: Vec::new(),
        }
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
            params: vec![ParamDef {
                name: "request".to_string(),
                type_ref: named_type(request),
            }],
            return_type: ReturnType::Result {
                ok: named_type(response),
                err: named_type(error),
            },
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
                    args: vec![TypeRef::Named {
                        name: "GenericError".to_string(),
                        args: vec![],
                    }],
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
                type_ref: TypeRef::Named {
                    name: "ProductAccountId".to_string(),
                    args: Vec::new(),
                },
                docs: None,
            },
            FieldDef {
                name: "context".to_string(),
                type_ref: TypeRef::Named {
                    name: "Bytes".to_string(),
                    args: Vec::new(),
                },
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
            }
            other => panic!("unexpected wrapper kind: {other:?}"),
        }

        match &latest.kind {
            VersionedKind::Tuple(TypeRef::Named { name, .. }) => {
                assert_eq!(name, "LatestRequest");
            }
            other => panic!("unexpected wrapper kind: {other:?}"),
        }

        match &fallback.kind {
            VersionedKind::Tuple(TypeRef::Named { name, .. }) => {
                assert_eq!(name, "IntermediateRequest");
            }
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
            &api(vec![
                request_method("later", Some(10)),
                subscription_method("stream", Some(2)),
            ]),
            2,
        )
        .expect("generate wire table");

        assert!(source.contains("export const EXAMPLE_STREAM = {"));
        assert!(source.contains("  trait: 200,"));
        assert!(source.contains("  method: 2,"));
        assert!(source.contains("export const EXAMPLE_LATER = {"));
        assert!(source.contains("  method: 10,"));
        assert!(
            source
                .find("export const EXAMPLE_STREAM")
                .expect("stream entry")
                < source
                    .find("export const EXAMPLE_LATER")
                    .expect("later entry")
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
            &api(vec![
                request_method("first", Some(2)),
                subscription_method("second", Some(2)),
            ]),
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
            message.contains("wire trait id 255 reused")
                && message.contains("reserved for protocol errors"),
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
            error.to_string().contains("wire trait id 255 reused")
                && error.to_string().contains("reserved for protocol errors"),
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
                        ok: TypeRef::Named {
                            name: "ExampleResponse".to_string(),
                            args: Vec::new(),
                        },
                        err: TypeRef::Named {
                            name: "ExampleError".to_string(),
                            args: Vec::new(),
                        },
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
            client_source.contains(
                "payload: T.VersionedExampleRequest.enc({ tag: \"V2\", value: request }),"
            )
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
                        ok: TypeRef::Named {
                            name: "ExampleResponse".to_string(),
                            args: Vec::new(),
                        },
                        err: TypeRef::Named {
                            name: "ExampleError".to_string(),
                            args: Vec::new(),
                        },
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
            client_source.contains(
                "payload: T.VersionedExampleRequest.enc({ tag: \"V1\", value: request }),"
            )
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
                        item: TypeRef::Named {
                            name: "ExampleItem".to_string(),
                            args: Vec::new(),
                        },
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
                        ok: TypeRef::Named {
                            name: "ExampleResponse".to_string(),
                            args: Vec::new(),
                        },
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
}
