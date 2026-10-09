//! Emits `wire_table.rs`: the (trait, method) discriminant lookup table the
//! server uses to pair incoming wire frames with their registered handler.
//!
//! A trait-level `#[wire_trait(id = N)]` annotation assigns the trait
//! discriminant; a per-method `#[wire(id = N)]` annotation assigns the method
//! discriminant. One id addresses a method regardless of its shape: which
//! leg of the exchange a frame carries (request/response, or a
//! subscription's start/stop/interrupt/receive) is named by the frame's
//! `message_type` byte, not by a separate id.
//!
//! Missing annotations and collisions (per trait) both hard-fail codegen.

use std::collections::BTreeMap;
use std::fmt::Write;

use anyhow::{Result, bail};
use indoc::{formatdoc, writedoc};

use crate::rustdoc::*;

use super::{const_name, wire_method_name};
use crate::RESERVED_PROTOCOL_ERROR_TRAIT_ID;

/// Wire discriminants for one method: the pair every frame it ever sends or
/// receives carries. Which leg a frame carries is its `message_type` byte;
/// the payload carries its own version.
#[derive(Debug, Clone, Copy)]
struct MethodIds {
    trait_id: u8,
    method_id: u8,
}

#[derive(Debug, Clone, Copy)]
enum MethodEntry {
    Request(MethodIds),
    Subscription(MethodIds),
}

impl MethodEntry {
    fn ids(self) -> MethodIds {
        match self {
            MethodEntry::Request(ids) | MethodEntry::Subscription(ids) => ids,
        }
    }
}

/// Emit the contents of `wire_table.rs`. `schema_hash` is the wire-contract
/// fingerprint emitted as `TRUAPI_WIRE_SCHEMA_HASH`, identical to the TS client's.
pub fn generate_wire_table(api: &ApiDefinition, schema_hash: &str) -> Result<String> {
    let mut method_entries: Vec<(String, MethodEntry)> = Vec::new();
    let mut seen: BTreeMap<(u8, u8), String> = BTreeMap::new();
    // Seed the reserved trait as already taken, so a trait declaring 255
    // collides here instead of silently claiming the address protocol errors
    // travel on. Reserving the trait rather than the single pair (255, 255) is
    // what makes this reachable: a method can only land on that pair through a
    // trait that owns 255, and nothing else constrains a declared trait id.
    let mut seen_traits: BTreeMap<u8, String> = BTreeMap::from([(
        RESERVED_PROTOCOL_ERROR_TRAIT_ID,
        "reserved for protocol errors".to_string(),
    )]);
    let mut seen_methods: BTreeMap<String, String> = BTreeMap::new();

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
            let entry = method_entry(trait_def, trait_id, method)?;
            let wire_method = wire_method_name(&trait_def.name, &method.name);
            if let Some(existing) = seen_methods.insert(
                wire_method.clone(),
                format!("{}::{}", trait_def.name, method.name),
            ) {
                bail!(
                    "wire method name `{wire_method}` reused: `{existing}` and `{}::{}` collide",
                    trait_def.name,
                    method.name
                );
            }
            insert_entry(&mut seen, &wire_method, entry)?;
            method_entries.push((wire_method, entry));
        }
    }

    method_entries.sort_by_key(|(_, entry)| {
        let MethodIds {
            trait_id,
            method_id,
        } = entry.ids();
        (trait_id, method_id)
    });

    render(&method_entries, schema_hash)
}

/// The trait's wire discriminant. Every API trait must carry a
/// `#[wire_trait(id = N)]` annotation; 255 is reserved for protocol errors
/// (that one is caught as a collision against the seeded reservation, not
/// here).
pub(super) fn trait_wire_id(trait_def: &TraitDef) -> Result<u8> {
    trait_def.wire_trait_id.ok_or_else(|| {
        anyhow::anyhow!(
            "trait `{}` is missing #[wire_trait(id = N)] annotation",
            trait_def.name
        )
    })
}

pub(super) fn method_wire_id(trait_def: &TraitDef, method: &MethodDef) -> Result<u8> {
    method.wire.id.ok_or_else(|| {
        anyhow::anyhow!(
            "method `{}::{}` is missing #[wire(id = N)] annotation",
            trait_def.name,
            method.name
        )
    })
}

fn method_entry(trait_def: &TraitDef, trait_id: u8, method: &MethodDef) -> Result<MethodEntry> {
    let method_id = method_wire_id(trait_def, method)?;
    let ids = MethodIds {
        trait_id,
        method_id,
    };
    match method.kind {
        MethodKind::Request => Ok(MethodEntry::Request(ids)),
        MethodKind::Subscription => Ok(MethodEntry::Subscription(ids)),
    }
}

fn insert_entry(
    seen: &mut BTreeMap<(u8, u8), String>,
    method_name: &str,
    entry: MethodEntry,
) -> Result<()> {
    let MethodIds {
        trait_id,
        method_id,
    } = entry.ids();
    if let Some(existing) = seen.insert((trait_id, method_id), method_name.to_string()) {
        bail!("wire id ({trait_id}, {method_id}) reused: `{existing}` and `{method_name}` collide");
    }
    Ok(())
}

fn render(methods: &[(String, MethodEntry)], schema_hash: &str) -> Result<String> {
    let mut out = String::new();
    writedoc!(
        out,
        r#"
        //! Wire-protocol discriminant table.
        //!
        //! Auto-generated by truapi-codegen. Do not edit.
        //!
        //! Every frame carries a `(trait, method)` discriminant pair; one
        //! method id addresses every frame a method ever sends or receives,
        //! regardless of shape. Which leg a frame carries (request/response,
        //! or a subscription's start/stop/interrupt/receive) is named by its
        //! `message_type` byte, and the payload carries its own version. The
        //! ids for each method are exposed as a named const
        //! (`PREIMAGE_SUBMIT`, ...); [`WIRE_TABLE`] and the generated
        //! dispatcher both reference those consts so the numbers live in
        //! exactly one place. The table is sorted by (trait id, method id).

        /// Wire discriminants for one method.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct MethodIds {{
            /// Trait discriminant carried by every frame of this method.
            pub trait_id: u8,
            /// Method discriminant carried by every frame of this method.
            pub method_id: u8,
        }}

        /// A single wire-table row.
        pub struct WireEntry {{
            /// Method name from the Rust trait.
            pub method: &'static str,
            /// What kind of slot this entry describes.
            pub kind: WireKind,
        }}

        /// Wire-slot shape: request/response or a subscription's
        /// start/stop/interrupt/receive quartet.
        pub enum WireKind {{
            /// Request/response method.
            Request(MethodIds),
            /// Subscription method.
            Subscription(MethodIds),
        }}
        "#
    )
    .unwrap();

    writedoc!(
        out,
        r#"
        /// Fingerprint of this build's wire contract: frame ids, method legs,
        /// sensitivity, and codec version, identical to the TS client's
        /// `TRUAPI_WIRE_SCHEMA_HASH`. A host stamps it on each debug envelope so
        /// the debugger refuses to decode a frame whose contract differs from
        /// its own, even when the coarse handshake codec version is unchanged.
        pub const TRUAPI_WIRE_SCHEMA_HASH: &str = "{schema_hash}";
        "#
    )
    .unwrap();

    // Per-method consts: the single source of truth for each method's ids.
    for (name, entry) in methods {
        let konst = const_name(name);
        let MethodIds {
            trait_id,
            method_id,
        } = entry.ids();
        let block = formatdoc! {
            r#"
            /// Wire discriminants for `{name}`.
            pub const {konst}: MethodIds = MethodIds {{
                trait_id: {trait_id},
                method_id: {method_id},
            }};
            "#
        };
        out.push('\n');
        out.push_str(&block);
    }

    out.push('\n');
    writedoc!(
        out,
        r#"
        /// The full wire table. Trait ids and per-trait method ordering are
        /// part of the wire protocol; only ever append within a trait.
        /// Removed methods leave their slot empty.
        pub const WIRE_TABLE: &[WireEntry] = &[
        "#
    )
    .unwrap();
    for (name, entry) in methods {
        let konst = const_name(name);
        let variant = match entry {
            MethodEntry::Request(_) => "Request",
            MethodEntry::Subscription(_) => "Subscription",
        };
        let block = formatdoc! {
            r#"
            WireEntry {{
                method: "{name}",
                kind: WireKind::{variant}({konst}),
            }},
            "#
        };
        for line in block.lines() {
            writeln!(out, "    {line}").unwrap();
        }
    }
    writeln!(out, "];").unwrap();

    Ok(out)
}
