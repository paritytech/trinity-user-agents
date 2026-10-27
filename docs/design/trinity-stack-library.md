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

A JavaScript project consumes stacks the way it consumes polkadot-api descriptors: `trinity-stack add` and
`trinity-stack update` write signed copies, and the dotNS contracts read from each stack's Asset Hub, into the project,
where the build imports them. At run time a stack reader starts from those copies and refreshes them by a policy the
project picks, from never to once per process to every so often, and never on every request.

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
- For JavaScript projects: the `trinity-stack` CLI that writes a project's stack copies, the stack reader that keeps
  them current, and reading a stack's dotNS contracts from its Asset Hub.

Out:

- Checks against live chains: genesis hashes, `NetworkSuffix`, the DUB attester. They need a chain connection, which the
  core and each tool already have.
- Anchoring a light client on a pinned checkpoint, which is the core's and truapi-provider's job.
- Checking a full spec against `genesisHash`, which means computing a trie root over the whole genesis storage. A node
  that loads the spec does that anyway.
- Tiers, import prompts, the stack picker and per-stack storage, which are host behaviour.
- A cache shared between tools or projects. Most tools run in CI and other fresh environments, where such a cache is
  empty every time, and state shared between unrelated tools surprises both. Each project keeps its own copies.
- Types generated from a stack's shape. Stacks drop services on purpose and differ from one another, so such types would
  be wrong the day a stack changes. A project states what it needs at run time instead (see `require` below).

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
- **`publish`** renders a model (a descriptor without `$v`, `sequence` and `generation`) and owns `sequence`, through
  the crate's `render`, which every publisher calls: this package, `truapi-host stack publish` and services such as
  testenv-manager. It keeps `PublishState { sequence, contentHash }`, leaves `sequence` alone when the content hash is
  unchanged, and otherwise raises it to the larger of the last value plus one and the time in seconds. With no state it
  uses the time, as the RFC says the CLI defaults to. `generation` is part of the content, so a new generation always
  raises `sequence`. Two publishes in the same second with no state get the same `sequence`, so an operator that
  publishes more often keeps the state.
- **`read`** fetches, calls `accept` and writes an updated copy back to a `Store`, keyed by `stack_id`. It adds no rules
  of its own, refuses redirects and gives up after a timeout.

## Consuming stacks

`read` fetches on every call. That suits a monitor, but a tool calling it per request would hit the stack's server once
per request, and a tool that has no copy at build time waits on that server at start. Projects need what polkadot-api
gives them for chains: a copy made at build time that ships with the project, and a choice of how far to trust it at run
time.

### At build time: `trinity-stack add` and `update`

The package ships a `trinity-stack` CLI, so a JavaScript project needs no `truapi-host` binary:

```
npx trinity-stack add <name> <link | url> [--key 0x..]
npx trinity-stack update [<name>...]
npx trinity-stack list
npx trinity-stack remove <name>
```

They keep one directory in the project, committed with it:

```
.trinity/
  stacks.json             { "previewnet": "polkadotapp://stack?key=0x4d43…&url=…", … }, edited by add and remove
  previewnet.json         the signed descriptor, byte for byte as served
  previewnet.dotns.json   the dotNS contracts read from previewnet's Asset Hub
  index.js, index.d.ts    generated: the stacks above, ready to import
```

- **`add`** records the link, then does what `update` does for that stack.
- **`update`** fetches each stack and runs `accept` with the committed copy as stored, so it refuses an unsigned stack,
  an expired one and anything that would lower `sequence`, as `truapi-host stack bundle` does. It then reads the stack's
  dotNS contracts (below) and regenerates the module. It exits 0 with nothing changed, 1 on an error and 2 when it wrote
  something, so a scheduled CI job can open a pull request whenever a stack moved and leave it to review.
- **The generated module** carries each stack as data, not as a file to load:

  ```ts
  export const stacks = {
    previewnet: { pointer: { url, key }, document: "<the signed JSON, as text>", dotns: { … } },
    …
  } as const;
  ```

  The document stays the exact text that was signed. Importing it as JSON and serialising it again would change the
  bytes, and `accept` compares stored and fetched copies byte for byte. The declaration file types `stacks` by name, so
  a project that refers to a stack it never added fails to compile. It does not type a stack's chains or services.

- **Committed, not generated on install.** The module is small and changes only when `update` runs, so committing it
  keeps a build reproducible without network access, and puts every change to a stack in front of a reviewer. A registry
  rewire, such as a devnet's registry moving to a new address, shows up as a diff.

`truapi-host stack bundle` stays for host builds that do not run Node. Both apply the same crate rules.

### dotNS contracts in the build copy

The RFC leaves dotNS out of the descriptor, since the chain holds it. Without it in the build copy, though, a tool would
make about 18 requests to Asset Hub on every run just to find the registrar. So `update` reads the contracts as the RFC
describes and writes, next to the signed copy:

```json
{
  "generation": "2026-10-26T22:19:25.356Z",
  "at": { "number": 182044, "hash": "0x…" },
  "dotns": { "dispatcher": "0x…", "protocolRegistry": "0x…", "addresses": { "registrar": "0x…", … } }
}
```

- Every read is made at one block, `at`, so the storage read and the contract calls see the same state, and anyone, a
  light client included, can check the record later against that block.
- `dotns` is null for a stack whose Asset Hub has no dotNS gateway.
- The record is not signed. It holds what a reader would read at run time from the RPC the signed descriptor names, so
  it adds no trust the descriptor did not already give, and the block makes it auditable.
- It belongs to one `generation`. A restart of the chains redeploys dotNS, so a record from another generation is never
  used.

### At run time: the stack reader

```ts
import { createStackReader } from "@parity/trinity-stack";
import { stacks } from "../.trinity/index.js";

const reader = createStackReader({
  bundled: stacks,
  refresh: "once", // "never" | "once" | { maxAgeMs }
  store, // optional, this project's own
});

const previewnet = await reader.get(stacks.previewnet, {
  require: { chains: ["AssetHub", "Bulletin"], services: ["eth-rpc"] },
});
const dotns = await reader.dotns(stacks.previewnet);
reader.onChange((change) => {
  /* a new sequence or generation: reconnect, drop chain caches */
});
await reader.refresh(stacks.previewnet);
```

The project picks how far to trust its build copy:

| `refresh`      | What `get` does                                                                                                                                          | For                                                           |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| `"never"`      | Uses the build copy, or the store's newer one. Fetches only on `refresh()`, or a stack it has no copy of.                                                | reproducible CI runs, apps that ship on a fixed set of stacks |
| `"once"`       | Fetches each stack the first time it is used in the process and keeps that answer for the rest of it. If the server is down, it uses the copy it has.    | CLIs, deploy jobs, test runs                                  |
| `{ maxAgeMs }` | Answers at once from the copy it has, and refreshes in the background when that copy is older than `maxAgeMs`. Waits only for a stack it has no copy of. | web apps, long-running services, hosts                        |

With `"never"`, a stack that moved on is the project's problem until it runs `update` and rebuilds, as with polkadot-api
descriptors that lag a runtime upgrade. It is a problem the project can name, though: the copy carries each chain's
genesis hash, and a chain client that connects to a chain reporting another one has found a restarted stack. The reader
never connects to a chain itself, so it leaves that check to the client, which already reads the genesis hash.

- **Every policy fetches at most once per stack at a time.** Callers that ask while a fetch is under way share it, and
  nothing fetches per request. A server sees one request per process with `"once"`, and one per reader per `maxAgeMs`
  otherwise.
- **Refreshes are conditional.** The reader keeps the `ETag` it was served and sends `If-None-Match`, so an unchanged
  stack costs a `304` with no body.
- **The copy it starts from** is the store's, if the project gave one and it holds a copy, otherwise the build's. Both
  go through `accept`, so the store can only move a stack forward from the build copy, never back.
- **The store is the project's own, and optional.** A web app passes one over `localStorage` or IndexedDB, so a reload
  starts from the last good copy. A service can keep everything in memory. A CLI needs none: it has the build copy, and
  a run lasts one process. The reader keeps, per stack, the document, when it was fetched and its `ETag`, and per stack
  the dotNS record, under keys of its own in that store.
- **`onChange`** reports each update with the old and new descriptor and whether the generation changed, so a
  long-running consumer reconnects without restarting.
- **`require`** checks that the stack has the chains and services the project needs, fails naming what is missing, and
  narrows the returned type, so `chain(previewnet, "AssetHub")` and `service(previewnet, "eth-rpc")` are a `Chain` and a
  `Service`, not possibly `undefined`. A tool that needs Bulletin fails at start on a stack without it, not at its first
  call.
- **`reader.dotns`** follows the descriptor. Under `"never"` it returns the build's record. Otherwise the record holds
  while the descriptor in use has the record's generation. A new generation means dotNS was redeployed, so the reader
  reads it again from the chain, through the `eth-rpc` the descriptor names or a `transport` the project passes, such as
  its light client, and keeps the answer as it keeps descriptors. A stack with no dotNS gateway is asked again on the
  same schedule, because a restarted stack sets its gateway after its chains start.
- **A pointer with no build copy**, such as a link a user pastes, works under every policy: the reader fetches it on
  first use.

`read`, `accept` and `resolveDotns` stay as the pieces the reader is built from, for a consumer that wants to manage
fetching itself.

### Who uses what

- **dotns-sdk, polkadot-cli and other CLIs.** `.trinity/` in the repository, refreshed by a scheduled `update` job that
  opens a pull request. Each release ships the stacks it was built with. At run time `refresh: "once"`, so a stack wiped
  since the release still works, at the cost of one small request per run. A `--pinned` flag, or
  `TRINITY_STACK_REFRESH=never`, gives a reproducible run. `--stack <link>` reads any other stack. Nothing is kept
  between runs.
- **bulletin-deploy and other deploy jobs.** `refresh: "once"`, since deploying to a wiped stack is the failure that
  matters. A job that must be reproducible pins instead and fails when the chain no longer matches its copy.
- **triangle-status and other web apps.** `.trinity/` built into the page, a store over `localStorage`, and
  `{ maxAgeMs: 3_600_000 }`. A page renders from the copy it has and updates when the refresh lands.
- **testenv-manager and other services.** `{ maxAgeMs }` in memory and `onChange`, to follow a wipe without a restart.
- **polkadot-pop-e2e and other test suites.** `"once"`, read at the start of the run. A suite that must not move during
  a run pins.
- **Hosts.** `truapi-host stack bundle` in the build, as the hosts RFC describes, and their own refresh schedule over
  the crate's `accept`.

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
- **dotNS without `eth-rpc`.** `update` and the reader call the registry through the `eth-rpc` a descriptor names. The
  host core calls it through Asset Hub's `ReviveApi_call` instead, which every stack has. Porting that call, with its
  SCALE encoding, would let a stack without `eth-rpc` but with a dotNS gateway resolve too. No such stack exists today.
- **Where the generated module lives.** `.trinity/index.js` is imported by path. polkadot-api instead installs its
  descriptors as a package (`"@polkadot-api/descriptors": "file:.papi/descriptors"`), which saves deep relative imports
  in a monorepo. Generating a `package.json` in `.trinity/` would allow both.
