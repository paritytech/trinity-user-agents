//! TypeScript code generation from extracted API definitions.

use std::{
	collections::{BTreeMap, BTreeSet, HashMap},
	fmt::Write,
	fs,
	path::Path,
};

use anyhow::{Result, bail};
use convert_case::{Case, Casing};
use indoc::{formatdoc, writedoc};

use crate::{RESERVED_PROTOCOL_ERROR_TRAIT_ID, rustdoc::*};

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
		NameMode::Generated { aliases } => {
			aliases.get(name).cloned().unwrap_or_else(|| name.to_string())
		},
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
					if (!internal_types.contains(name), inner_version) >
						(!internal_types.contains(&selected.1), selected.0) =>
				{
					*selected = (inner_version, name.clone());
				},
				Some(_) => {},
				None => {
					selected_by_base.insert(base.to_string(), (inner_version, name.clone()));
				},
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
		let entry = selected_by_base.entry(base.to_string()).or_insert((version, ty.name.clone()));
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
			if version_prefixed_type(&ty.name).is_some() &&
				!aliases.contains_key(&ty.name) &&
				!names.contains(&ty.name)
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
		VersionedKind::Unit => {},
		VersionedKind::Tuple(inner) => {
			preserve_version_prefixed_names(type_ref_dependencies(inner), aliases, names);
		},
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
			},
			_ => return None,
		};
		version_variants.insert(version, VersionedWrapperVariant { version, kind });
	}

	Some(VersionedWrapper { variants: version_variants })
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
	if let TypeRef::Named { name, args } = ty &&
		args.is_empty() &&
		let Some(wrapper) = wrappers.get(name)
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
	fs::write(Path::new(output_dir).join("internal-client.ts"), internal_client)?;

	let index_code = generate_index();
	fs::write(Path::new(output_dir).join("index.ts"), index_code)?;

	let wire_table_code = generate_wire_table(api, target_version)?;
	fs::write(Path::new(output_dir).join("wire-table.ts"), wire_table_code)?;

	let decode_table_code = generate_decode_table(api, target_version)?;
	fs::write(Path::new(output_dir).join("wire-decode.ts"), decode_table_code)?;

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
			bail!("wire trait id {trait_id} reused: `{existing}` and `{}` collide", trait_def.name);
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
			let wire_kind = match method.wire_kind() {
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
		anyhow::anyhow!("trait `{}` is missing #[wire_trait(id = N)] annotation", trait_def.name)
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
		(RESERVED_PROTOCOL_ERROR_TRAIT_ID, RESERVED_PROTOCOL_ERROR_TRAIT_ID),
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
	let (response, error) = method.return_type.response_types();
	let shape = match method.wire_kind() {
		MethodKind::Request => "res",
		MethodKind::Subscription => "sub",
	};
	let _ = write!(
		out,
		"{shape}<{},{}>",
		type_signature(response, types, &mut Vec::new()),
		type_signature(error, types, &mut Vec::new())
	);
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
		},
		TypeRef::Tuple(items) => {
			let inner: Vec<String> =
				items.iter().map(|item| type_signature(item, types, seen)).collect();
			format!("({})", inner.join(","))
		},
		TypeRef::Named { name, args } => {
			if name == "ProductId" {
				return "str".to_string();
			}
			let rendered_args: Vec<String> =
				args.iter().map(|arg| type_signature(arg, types, seen)).collect();
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
				},
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
				},
				TypeDefKind::TupleStruct(items) => {
					let rendered: Vec<String> =
						items.iter().map(|item| type_signature(item, types, seen)).collect();
					format!("({})", rendered.join(","))
				},
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
								},
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
								},
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
				},
			};
			seen.pop();
			format!("{name}{suffix}{body}")
		},
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
		canonical.push_str(&format!("{trait_id}.{method_id}:{tag}:{initiator}:{payload}\n"));
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
	Ok(wrapper_names.is_empty() || method_wire_version(method, wrappers, target_version)?.is_some())
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
			Some(current) => {
				current.into_iter().filter(|version| versions.contains(version)).collect()
			},
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
		},
		ReturnType::Subscription { item, interrupt } => {
			collect_type_versioned_wrappers(item, wrappers, &mut names);
			collect_type_versioned_wrappers(
				call_error_inner(interrupt).unwrap_or(interrupt),
				wrappers,
				&mut names,
			);
		},
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
		},
		TypeRef::Vec(inner) | TypeRef::Option(inner) | TypeRef::Array(inner, _) => {
			collect_type_versioned_wrappers(inner, wrappers, names);
		},
		TypeRef::Tuple(items) => {
			for item in items {
				collect_type_versioned_wrappers(item, wrappers, names);
			}
		},
		TypeRef::Primitive(_) | TypeRef::Generic(_) | TypeRef::Unit => {},
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
			internal_types.contains(&ty.name) == internal &&
				(version_prefixed_type(&ty.name).is_none() ||
					aliases.contains_key(&ty.name) ||
					preserved_version_prefixed_types.contains(&ty.name))
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
	if dependencies.iter().any(|(_, needs_hex_string)| *needs_hex_string) {
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
		methods.iter().any(|method| method.wire_kind() == MethodKind::Subscription)
	});
	let has_host_requests = services.iter().any(|(_, methods)| {
		methods
			.iter()
			.any(|method| method.wire.host_initiated && method.kind == MethodKind::Request)
	});
	let has_host_methods = services
		.iter()
		.any(|(_, methods)| methods.iter().any(|method| method.wire.host_initiated));
	let hex_import =
		if uses_hex_string { "import type { HexString } from '../scale.js';" } else { "" };
	let registration_import = [
		has_host_methods.then_some("HostInitiatedSubscriptionRegistration, "),
		has_host_requests.then_some("HostInitiatedRequestHandler, "),
	]
	.into_iter()
	.flatten()
	.collect::<String>();
	let method_ids_import = if has_subscriptions { "MethodIds, " } else { "" };
	let request_adapter_import = if has_host_requests { ", hostRequestHandler" } else { "" };

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
        import {{ SubscriptionError{request_adapter_import} }} from '../transport.js';
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
		for method in methods.iter().copied().filter(|method| method.wire.host_initiated) {
			emit_host_initiated_field(&mut out, method, &wrappers, target_version)?;
		}
		writeln!(out, "  constructor(transport: TrUApiTransport) {{").unwrap();
		if uses_transport {
			writeln!(out, "    this.#transport = transport;").unwrap();
		}
		for method in methods.iter().copied().filter(|method| method.wire.host_initiated) {
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

	let client_type = if internal { "InternalTrUApiClient" } else { "TrUApiClient" };
	writeln!(out, "export interface {client_type} {{").unwrap();
	for (trait_def, _) in &services {
		let field = to_camel_case(&trait_def.name);
		let namespace_type = format!("{}Client", trait_def.name);
		let namespace_type =
			if internal { format!("Readonly<{namespace_type}>") } else { namespace_type };
		writeln!(out, "  readonly {field}: {namespace_type};").unwrap();
	}
	writeln!(out, "}}\n").unwrap();
	if !internal {
		writeln!(out, "export type Client = TrUApiClient;\n").unwrap();
	}
	let factory = if internal { "createInternalClient" } else { "createClient" };
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
				Some(name) => {
					format!("(payload) => T.{}.dec(payload)", versioned_wrapper_ts_name(name))
				},
				None => "() => undefined".to_string(),
			};

			let mut lines = Vec::new();
			let (response, error) = method.return_type.response_types();
			let response_codec = leg_codec_expr(response, &wrappers)?;
			let error_codec = leg_error_codec_expr(error, &wrappers, &ctx)?;
			lines.push(format!("    0: {request_decoder},"));
			match method.wire_kind() {
				MethodKind::Request => {
					lines.push(format!(
						"    1: (payload) => S.Result({response_codec}, {error_codec}).dec(payload),"
					));
				},
				MethodKind::Subscription => {
					lines.push(format!("    1: (payload) => {response_codec}.dec(payload),"));
					lines.push(format!(
						"    2: (payload) => S.Result(S._void, {error_codec}).dec(payload),"
					));
					lines.push("    3: () => undefined,".to_string());
				},
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
	if params.len() == 1 &&
		let Some((wrapper_name, wrapper)) = versioned_wrapper_for(&params[0].type_ref, wrappers)
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
			},
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
			VersionedKind::Unit => Ok(ResponseEmission { inner_type_ts: "undefined".to_string() }),
			VersionedKind::Tuple(inner) => {
				Ok(ResponseEmission { inner_type_ts: ts_type_qualified(inner)? })
			},
		};
	}

	Ok(ResponseEmission { inner_type_ts: ts_type_qualified(ty)? })
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

	let inner_type_ts =
		format!("S.CallErrorValue<{}>", ts_type_qualified_preserve(error_wrapper_ty)?);
	Ok(ResponseEmission { inner_type_ts })
}

/// The codec expression that decodes this leg's own wire payload directly: the
/// recognized wrapper's `Versioned{Name}` codec, whose `V<N>` tag is the wire's
/// only version signal. Every method's request, response and item is one of
/// these, an empty payload included; anything else has no describable codec.
fn leg_codec_expr(ty: &TypeRef, wrappers: &BTreeMap<String, VersionedWrapper>) -> Result<String> {
	match versioned_wrapper_for(ty, wrappers) {
		Some((wrapper_name, _)) => Ok(format!("T.{}", versioned_wrapper_ts_name(wrapper_name))),
		None => {
			bail!(
				"type `{}` is not a recognized versioned wrapper, \
             so no wire codec can be derived for it",
				ts_type_name_hint(ty)
			)
		},
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
		return Ok(format!("S.CallError(T.{})", versioned_wrapper_ts_name(wrapper_name)));
	}
	Ok(format!("S.CallError({})", codec_expr_mode(error_wrapper_ty, true, ctx, NameMode::Public)?))
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
	Ok(format!("S.indexedTaggedUnion({{{entries}}})", entries = entries.join(", ")))
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
	write_jsdoc(out, "  ", method.docs.as_deref());

	if method.wire.host_initiated {
		return emit_host_initiated_method(out, method, wrappers, target_version);
	}
	let payload = emit_payload(&method.params, wrappers, wire_version)?;

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
		bail!("method `{}`: request parameter is not a recognized versioned wrapper", method.name);
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
		},
		(MethodKind::Subscription, ReturnType::Subscription { item, interrupt }) => {
			let response = emit_response(item, wrappers, wire_version)?;
			let item_codec = leg_codec_expr(item, wrappers)?;
			let item_is_wrapper = versioned_wrapper_for(item, wrappers).is_some();
			let error = emit_error_response(interrupt, wrappers, wire_version)?;
			let error_codec = leg_error_codec_expr(interrupt, wrappers, &ctx)?;
			emit_subscribe_method(
				out,
				SubscribeMethod {
					ts_method_name: &ts_method_name,
					wire_const: &wire_const,
					payload: &payload,
					item_type_ts: response.inner_type_ts.clone(),
					item_codec,
					item_is_wrapper,
					err: error,
					error_codec,
					version,
					request_name,
				},
			)?;
		},
		(kind, return_type) => {
			bail!(
				"Generator internal mismatch for method `{}`: kind {:?} does not match return type {:?}",
				method.name,
				kind,
				return_type
			);
		},
	}

	Ok(())
}

fn host_registration_field(method: &MethodDef) -> String {
	format!("#{}Registration", to_camel_case(&strip_prefix(&method.name)))
}

fn host_request_inner_type(
	ty: &TypeRef,
	wrappers: &BTreeMap<String, VersionedWrapper>,
	version: u32,
) -> Result<String> {
	let (name, wrapper) = versioned_wrapper_for(ty, wrappers)
		.ok_or_else(|| anyhow::anyhow!("host request payload is not a versioned wrapper"))?;
	let variant = wrapper
		.variants
		.get(&version)
		.ok_or_else(|| anyhow::anyhow!("versioned wrapper `{name}` has no V{version} variant"))?;
	Ok(match &variant.kind {
		VersionedKind::Unit => "undefined".to_string(),
		VersionedKind::Tuple(_) => format!(
			"Extract<T.{}, {{ tag: \"V{version}\" }}>[\"value\"]",
			versioned_wrapper_ts_name(name),
		),
	})
}

fn emit_host_initiated_types(
	method: &MethodDef,
	wrappers: &BTreeMap<String, VersionedWrapper>,
	target_version: u32,
) -> Result<(PayloadEmission, ResponseEmission, ResponseEmission, u32)> {
	let wire_version = method_wire_version(method, wrappers, target_version)?.ok_or_else(|| {
		anyhow::anyhow!("host-initiated method `{}` is not versioned", method.name)
	})?;
	let mut payload = emit_payload(&method.params, wrappers, Some(wire_version))?;
	let (response_type, error_type) = method.return_type.response_types();
	let mut response = emit_response(response_type, wrappers, Some(wire_version))?;
	let error = emit_error_response(error_type, wrappers, Some(wire_version))?;
	if method.kind == MethodKind::Request {
		let request = method.params.first().ok_or_else(|| {
			anyhow::anyhow!("host request `{}` has no request parameter", method.name)
		})?;
		payload.inner_type_ts = host_request_inner_type(&request.type_ref, wrappers, wire_version)?;
		response.inner_type_ts = host_request_inner_type(response_type, wrappers, wire_version)?;
	}
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
	let (response, error) = method.return_type.response_types();
	let Some(request_name) = request_wrapper_name(method, wrappers) else {
		bail!(
			"host-initiated method `{}`: request parameter is not a recognized versioned wrapper",
			method.name
		);
	};
	let Some((response_name, _)) = versioned_wrapper_for(response, wrappers) else {
		bail!(
			"host-initiated method `{}`: response type is not a recognized versioned wrapper",
			method.name
		);
	};
	let request_codec = format!("T.{}", versioned_wrapper_ts_name(request_name));
	let response_codec = format!("T.{}", versioned_wrapper_ts_name(response_name));
	let error_codec = leg_error_codec_expr(error, wrappers, &ctx)?;
	let field = host_registration_field(method);
	let single_response = if method.kind == MethodKind::Request {
		format!("\n  singleResponse: {{ requestVersion: {version} }},")
	} else {
		String::new()
	};
	let decode_request = if method.kind == MethodKind::Request {
		formatdoc!(
			r#"(payload) => {{
                const request = {request_codec}.dec(payload);
                if (request.tag !== "V{version}") throw new Error("unsupported request version");
                return request.value;
              }}"#,
		)
	} else {
		format!("(payload) => {request_codec}.dec(payload).value")
	};
	writedoc!(
		out,
		r#"
        this.{field} = transport.registerHostInitiatedSubscription({{
          ids: W.{wire_const},{single_response}
          decodeRequest: {decode_request},
          encodeItem: (item) => {response_codec}.enc({{ tag: "V{version}", value: item }}),
          encodeInterrupt: interruptEncoder({error_codec}),
          declinePayload: HOST_INITIATED_DECLINE_PAYLOAD,
          bufferCapacity: HOST_INITIATED_BUFFER_CAPACITY,
        }});
        "#
	)
	.unwrap();
	Ok(())
}

fn emit_host_initiated_method(
	out: &mut String,
	method: &MethodDef,
	wrappers: &BTreeMap<String, VersionedWrapper>,
	target_version: u32,
) -> Result<()> {
	let (payload, response, error, _) =
		emit_host_initiated_types(method, wrappers, target_version)?;
	let kind = match method.kind {
		MethodKind::Request => "Request",
		MethodKind::Subscription => "Subscription",
	};
	let handler =
		if method.kind == MethodKind::Request { "hostRequestHandler(handler)" } else { "handler" };
	let name = format!("on{}", strip_prefix(&method.name).to_case(Case::Pascal));
	writedoc!(
		out,
		"
          {name}(
            handler: HostInitiated{kind}Handler<{request}, {response}, {reason}>,
          ): {{ unsubscribe(): void }} {{
            return this.{field}.setHandler({handler});
          }}
        ",
		request = payload.inner_type_ts,
		response = response.inner_type_ts,
		reason = error.inner_type_ts,
		field = host_registration_field(method),
	)
	.unwrap();
	Ok(())
}

struct SubscribeMethod<'a> {
	ts_method_name: &'a str,
	wire_const: &'a str,
	payload: &'a PayloadEmission,
	item_type_ts: String,
	item_codec: String,
	item_is_wrapper: bool,
	err: ResponseEmission,
	error_codec: String,
	version: u32,
	request_name: Option<&'a str>,
}

/// Emits a subscribe method body that returns an Observable-compatible
/// object. `Start`'s payload is the request wrapper's own encoding, or empty
/// bytes when the method takes no request at all. The `Interrupt` payload is
/// `Result<(), CallError<Err>>`, so every generated method decodes `Ok(())`
/// as a clean completion and `Err(reason)` as `error`.
fn emit_subscribe_method(out: &mut String, method: SubscribeMethod<'_>) -> Result<()> {
	let SubscribeMethod {
		ts_method_name,
		wire_const,
		payload,
		item_type_ts,
		item_codec,
		item_is_wrapper,
		err,
		error_codec,
		version,
		request_name,
	} = method;
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
		Some(name) => {
			format!(
				"T.{}.enc({{ tag: \"V{version}\", value: {} }})",
				versioned_wrapper_ts_name(name),
				payload.value_expr
			)
		},
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
	if ty.name == "ProductId" {
		writedoc!(
			out,
			r#"
            declare const productIdBrand: unique symbol;
            /** A product identifier. Rust validates identifiers received by the host. */
            export type ProductId = string & {{ readonly [productIdBrand]: true }};
            /** Designate a product identifier without changing or validating its value. */
            export function asProductId(value: string): ProductId {{
              return value as ProductId;
            }}
            "#,
		)
		.unwrap();
		return Ok(());
	}

	write_jsdoc(out, "", ty.docs.as_deref());
	match &ty.kind {
		TypeDefKind::Alias(type_ref) => {
			writeln!(
				out,
				"export type {emitted_name}{generic_decl} = {};",
				ts_type_with_named(type_ref, false, generated_names)?
			)
			.unwrap();
		},
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
		},
		TypeDefKind::TupleStruct(fields) => {
			writeln!(
				out,
				"export type {emitted_name}{generic_decl} = {};",
				unnamed_fields_type_mode(fields, false, generated_names)?
			)
			.unwrap();
		},
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
					writeln!(out, "  | {}", enum_variant_ts_type_mode(variant, generated_names)?)
						.unwrap();
				}
				writeln!(out, ";").unwrap();
			}
		},
	}

	Ok(())
}

fn write_codec_definition(
	out: &mut String,
	ty: &TypeDef,
	emit_versions: &BTreeMap<String, BTreeSet<u32>>,
	aliases: &BTreeMap<String, String>,
) -> Result<()> {
	if ty.name == "ProductId" {
		writeln!(
			out,
			"export const ProductId = S.enhanceCodec(S.str, (value: ProductId) => value, asProductId);"
		)
		.unwrap();
		return Ok(());
	}
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
		let emitted_name = aliases.get(&ty.name).map(String::as_str).unwrap_or(&ty.name);
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
		aliases.get(&ty.name).map(String::as_str).unwrap_or(&ty.name),
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
	let function_name = aliases.get(&ty.name).map(String::as_str).unwrap_or(&ty.name);
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
	detect_versioned_wrapper(ty).is_some() &&
		(emit_versions.contains_key(&ty.name) || aliases.values().any(|alias| alias == &ty.name))
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
		},
		TypeDefKind::TupleStruct(fields) => {
			unnamed_fields_codec_expr_mode(fields, qualified, ctx, mode)
		},
		TypeDefKind::Enum(variants) => {
			if is_unit_only_enum(ty) {
				unit_enum_codec_expr(type_name, variants)
			} else if variants.iter().any(|variant| variant.codec_index.is_some()) {
				// `S.TaggedUnion` numbers variants by key order, so an enum that
				// pins its SCALE indexes gets them written out.
				let variants = variants
					.iter()
					.enumerate()
					.map(|(position, variant)| {
						Ok(format!(
							"{}: [{}, {}] as const",
							variant.name,
							scale_discriminant(variant, position),
							variant_codec_expr_mode(&variant.fields, qualified, ctx, mode)?
						))
					})
					.collect::<Result<Vec<_>>>()?
					.join(", ");
				Ok(format!("S.indexedTaggedUnion({{{variants}}})"))
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
		},
	}
}

fn is_unit_only_enum(ty: &TypeDef) -> bool {
	detect_versioned_wrapper(ty).is_none() &&
		matches!(
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

/// The SCALE discriminant of the variant at `position`: its
/// `#[codec(index = N)]` when it carries one, its position otherwise.
fn scale_discriminant(variant: &VariantDef, position: usize) -> usize {
	variant.codec_index.map_or(position, usize::from)
}

fn unit_enum_codec_expr(type_name: &str, variants: &[VariantDef]) -> Result<String> {
	// `S.Status` numbers values by position, so a pinned index must match it.
	if let Some((position, variant)) = variants
		.iter()
		.enumerate()
		.find(|(position, variant)| scale_discriminant(variant, *position) != *position)
	{
		anyhow::bail!(
			"unit-only enum `{type_name}` pins `{}` to index {} at position {position}; \
			 declare its variants in index order",
			variant.name,
			scale_discriminant(variant, position),
		);
	}
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
		fields => {
			format!(
				"{{ tag: \"{}\"; value: {} }}",
				variant.name,
				variant_value_type_mode(fields, mode)?
			)
		},
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
		},
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
			Ok(format!("{}: {}", name, codec_expr_mode(&field.type_ref, qualified, ctx, mode)?))
		})
		.collect::<Result<Vec<_>>>()?
		.join(", ");
	Ok(format!("S.Struct({{{field_specs}}}) as S.Codec<{type_name}>"))
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
			let target = if qualified { qualify_named(&resolved, mode) } else { resolved };

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
		},
		TypeRef::Vec(inner) => match inner.as_ref() {
			TypeRef::Primitive(name) if name == "u8" => Ok("S.Hex()".to_string()),
			_ => Ok(format!("S.Vector({})", codec_expr_mode(inner, qualified, ctx, mode)?)),
		},
		TypeRef::Option(inner) => {
			Ok(format!("S.Option({})", codec_expr_mode(inner, qualified, ctx, mode)?))
		},
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
		},
		TypeRef::Array(inner, len) => match inner.as_ref() {
			TypeRef::Primitive(name) if name == "u8" => Ok(format!("S.Hex({len})")),
			_ => {
				Ok(format!("S.Vector({}, {})", codec_expr_mode(inner, qualified, ctx, mode)?, len))
			},
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
			let target = if qualified { qualify_named(&resolved, mode) } else { resolved };

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
		},
		TypeRef::Vec(inner) => match inner.as_ref() {
			TypeRef::Primitive(name) if name == "u8" => Ok(hex_string_ts_name(qualified)),
			_ => Ok(format!("Array<{}>", ts_type_with_named(inner, qualified, mode)?)),
		},
		TypeRef::Option(inner) => {
			Ok(format!("{} | undefined", ts_type_with_named(inner, qualified, mode)?))
		},
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
		},
		TypeRef::Array(inner, _len) => match inner.as_ref() {
			TypeRef::Primitive(name) if name == "u8" => Ok(hex_string_ts_name(qualified)),
			_ => Ok(format!("Array<{}>", ts_type_with_named(inner, qualified, mode)?)),
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
	let optional = matches!(ty, TypeRef::Option(_)) ||
		matches!(ty, TypeRef::Primitive(name) if name == "optionBool");
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
	if params.is_empty() { String::new() } else { format!("<{}>", params.join(", ")) }
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

	if display_name == "JSON RPC" { "JSON-RPC".to_string() } else { display_name }
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
mod tests;
