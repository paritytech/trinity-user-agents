---
title: "Trinity stack library"
type: design
status: draft
author: mordamax
---

# Trinity stack library

_How the reference implementation of [RFC: Trinity stack descriptor](../rfcs/trinity-stack-descriptor.md) is built. The
RFC is normative; this page explains how the code is split and why._

## Summary

One Rust crate, [`trinity-stack`](../../rust/crates/trinity-stack), holds every rule a reader applies: parsing,
validation, signatures, links, and what a reader does with a newly fetched descriptor. The host core and the
`truapi-host` CLI link it directly, and `@parity/trinity-stack` ships it compiled to wasm as a library for JavaScript
tools, adding I/O and publishing. Conformance vectors live with the crate, and every implementation runs them.

It lands as a stack: the crate, then the package and the CLI commands on top of it. The host checks RFC 2 needs, the
reserved values and the values a build pins, land with that RFC.

Every rule exists once. The RFC's strict rules (I-JSON duplicates, strict Ed25519, known fields only, rollback) are easy
to get subtly different in two languages, and a reader that accepts what another refuses is the failure the vectors
exist to catch. Compiling the one crate to wasm removes the second implementation instead of testing it.

## Scope

In:

- Parse and validate a descriptor or an envelope, `$v: 1`, and verify it against a pointer.
- Encode and decode pointer links, and tell whether two pointers name the same stack.
- Decide what a reader does with a fetched descriptor, given what it stored.
- Check a spec's bytes against `sha256`, and a light spec against its chain's `genesisHash`.
- Sign, render a model, track `sequence` across publishes, and encode links. Drawing a link's QR code is left to the
  consumer.
- `truapi-host stack` commands around all of it, the vectors and a JSON Schema.

Out:

- Checks against live chains: genesis hashes, `NetworkSuffix`, the DUB attester. They need a chain connection, which the
  core and each tool already have.
- Anchoring a light client on a pinned checkpoint, which is the core's and truapi-provider's job.
- Checking a full spec against `genesisHash`, which means computing a trie root over the whole genesis storage. A node
  that loads the spec does that anyway.
- Tiers, import prompts, the stack picker and per-stack storage, which are host behaviour.

## The crate

The crate has no I/O. Fetching, storage and clocks belong to the caller, which keeps it usable in the core, in wasm and
in tests.

- **Parsing.** `Document::parse` reads I-JSON first: serde_json keeps the last of two equal keys and skips ignored
  fields unread, so the one pass that builds the document refuses a repeated member or a noncharacter wherever it
  appears. It then drops service entries of unknown kinds and lets serde ignore unknown fields, and only then checks the
  format rules on what remains.
- **Pointers.** `open(bytes, pointer)` applies the pointer's rules: a keyed pointer accepts only an envelope that
  verifies, and an unkeyed one reads an envelope as unsigned.
- **Signatures.** Strict RFC 8032: canonical `A` and `R`, neither of small order, `S` below the group order, then
  `ed25519-dalek`'s cofactorless `verify_strict`. `verify_strict` does not refuse a non-canonical `A` on its own, so the
  crate checks the encodings first. The workspace's `ed25519-zebra` follows ZIP 215 and accepts what the RFC refuses, so
  it is not used here.
- **Reader state.** `accept(pointer, stored, fetched, now)` takes the stored bytes and the fetched ones and returns the
  bytes to store next, the descriptor to use and a status. A stored copy that no longer passes counts as absent.

| Status        | When                                                                                 |
| ------------- | ------------------------------------------------------------------------------------ |
| `updated`     | verified, and a higher `sequence` than the stored one, or any `sequence` if unsigned |
| `unchanged`   | same `sequence`, same content                                                        |
| `conflict`    | same `sequence`, different content; the stored copy is kept                          |
| `rollback`    | signed, and a lower `sequence`; refused, the stored copy is kept                     |
| `unreachable` | fetch failed; the stored copy is kept                                                |
| `refused`     | the fetched bytes break a rule; the stored copy is kept                              |

It also reports a new generation, so the caller drops its chain caches, and whether the copy is past `validUntil` or
`expiresAt`. A host seeds `stored` from its bundled copy, so a stack never waits on its server.

- **Specs.** A light spec carries the genesis state root, so `light_spec_genesis` rebuilds the genesis header from it
  (number 0, zero parent, empty extrinsics root, empty digest) and hashes it. The tests check it against the light specs
  and genesis hashes the provider catalog ships.
- **Identity.** `Pointer::stack_id` is the key in hex, or the URL normalised without its fragment when there is no key.
  `same_stack` compares it, and readers key what they store by it.
- **Errors.** One variant per rule. `Error::code` is the stable name the vectors use, so a vector fails on the wrong
  reason as well as on the wrong verdict.

## The package

The package loads the wasm once at import, so its API is synchronous: Node and Bun read the file, browsers fetch it next
to the module. Values cross the boundary as plain objects of the crate's own types, whose TypeScript declarations tsify
generates, and errors as a `TrinityStackError` with the rule's `code`.

- **Signing** runs in the crate too. That keeps one Ed25519 implementation and adds no JavaScript dependency.
- **`publish`** renders a model (a descriptor without `$v`, `sequence` and `generation`) and owns `sequence`, through the
  crate's `render`, which every publisher calls: this package, `truapi-host stack publish` and services such as
  testenv-manager. It keeps `PublishState { sequence, contentHash }`, leaves `sequence` alone when the content hash is unchanged, and otherwise
  raises it to the larger of the last value plus one and the time in seconds. With no state it uses the time, as the RFC
  says the CLI defaults to. `generation` is part of the content, so a new generation always raises `sequence`. Two
  publishes in the same second with no state get the same `sequence`, so an operator that publishes more often keeps the
  state.
- **`read`** fetches, calls `accept` and writes an updated copy back to a `Store`, keyed by `stack_id`. It adds no rules
  of its own, refuses redirects and gives up after a timeout.

## CLI

The commands join the existing `truapi-host` CLI, which already links Rust crates and ships prebuilt binaries:

```
truapi-host stack keygen  --out key.hex
truapi-host stack publish <model.json> --generation <g> --out <dir> [--signing-key-file <file>] [--state <file>]
truapi-host stack verify  <link | url | file> [--key 0x..]
truapi-host stack bundle  <link | url> [--key 0x..] --out <file>
truapi-host stack link    <url> [--key 0x..]
```

- `publish` writes `<dir>/.well-known/trinity-stack.json`, ready for a static host, and updates the state file.
- `bundle` is the build step of the host RFC. A Built-in stack must be signed, so it refuses a pointer without a key, an
  expired copy, and anything that would lower the bundled `sequence`.
- A secret key is only ever read from a file, never from an argument, so it stays out of shell history.

## Conformance vectors

`rust/crates/trinity-stack/tests/vectors.rs` generates `vectors/` and fails when the committed files differ from what it
generates, so they cannot drift. Each case names the expected verdict or rule code:

- **Documents**: valid descriptors (minimal, full, private-range, unknown fields and kinds, a fork, signed), I-JSON
  failures, every format rule, envelope rules, and each signature rule, including a signature valid only under the
  cofactored equation.
- **Reader**: each status for signed and unsigned stacks, a new generation, past `validUntil`, past `expiresAt`.
- **Links**: the RFC's example, other schemes, a missing or repeated `url`, bad URLs and keys.

The Rust tests and the package's bun tests both run the committed files. A reader outside this repository can run them
too. `schema/trinity-stack.v1.schema.json` describes the shape; rules that span fields stay with the vectors.

## Open questions

- **Where it lives.** This repository keeps the crate next to its first reader, the core. A separate repository would
  suit operators better if the core's release cadence gets in the way.
- **wasm size.** The bundle is about 540 KiB before `wasm-opt`. Fine for tools; a page that only reads stacks may want a
  build without signing.
- **Light-spec building.** Appendix A of the RFC plans to build light specs from full specs. That needs a node, so it is
  not part of the CLI.
