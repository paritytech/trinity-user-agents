# trinity-stack

Reads, verifies and signs Trinity stack descriptors, as the
[Trinity stack descriptor RFC](https://github.com/paritytech/trinity-user-agents/blob/main/docs/rfcs/trinity-stack-descriptor.md)
defines them. A descriptor is one JSON file that says which chains and services make up a stack, such as previewnet, so
hosts and tools read the same facts instead of keeping their own copies.

The crate holds every rule a reader applies, so the host core, `truapi-host stack` and `@parity/trinity-stack` (this
crate compiled to wasm) accept and refuse the same descriptors. It has no I/O: callers fetch, store and keep time.

## Adding it to a project

The crate is on [crates.io](https://crates.io/crates/trinity-stack) with the same version as `@parity/trinity-stack`:

```toml
[dependencies]
trinity-stack = "0.2"
```

## Reading a stack

```rust,no_run
use trinity_stack::{Fetched, Pointer, accept};

# fn fetch(_: &str) -> Option<Vec<u8>> { None }
# fn load() -> Option<Vec<u8>> { None }
# fn save(_: &[u8]) {}
# fn now_unix_seconds() -> i64 { 0 }
let pointer = Pointer::from_link("polkadotapp://stack?key=0x3b6a…&url=https%3A%2F%2F…")?;
let fetched = fetch(pointer.url().as_str());
let stored = load(); // the last `accepted.document`, or the copy a build bundles
let accepted = accept(
	&pointer,
	stored.as_deref(),
	fetched.as_deref().map_or(Fetched::Unreachable, Fetched::Document),
	now_unix_seconds(),
)?;
save(&accepted.document); // keyed by `pointer.stack_id()`
let chains = &accepted.stack.descriptor.chains;
# Ok::<(), trinity_stack::Error>(())
```

`accept` refuses a signed descriptor with a lower `sequence` than the stored one, keeps the stored copy when the fetch
fails or breaks a rule, and reports a new `generation` (drop cached chain data), `stale` (past `validUntil`) and
`expired` (past `expiresAt`).

## Known stacks

The crate lists the links of the Trinity stacks below. A link with a key is a signed stack; host builds bundle only
signed stacks, from their copies in the repository's `stacks/`. `known_stack` returns one's pointer by name, and
`known_stack_links` lists every name and link:

```rust
let pointer = trinity_stack::known_stack("previewnet").expect("unknown stack previewnet");
assert_eq!(pointer.url().host_str(), Some("previewnet.substrate.dev"));
```

| Name | Stack | Signed | Descriptor |
| --- | --- | --- | --- |
| `paseo-next-v2` | Paseo Next v2 | yes | <https://stack-paseo-next.parity-testnet.parity.io/.well-known/trinity-stack.json> |
| `pcf-devnet` | PCF Devnet | yes | <https://stack.api.polkadotcommunity.foundation/.well-known/trinity-stack.json> |
| `polkadot` | Polkadot | yes | <https://stack.api.pcf.network/.well-known/trinity-stack.json> |
| `previewnet` | Previewnet | yes | <https://previewnet.substrate.dev/.well-known/trinity-stack.json> |
| `westend` | Westend | no | <https://stack-westend.parity-testnet.parity.io/.well-known/trinity-stack.json> |

To add a stack to the list, open a pull request that:

1. adds its name and link to
   [`known-stacks.json`](https://github.com/paritytech/trinity-user-agents/blob/main/rust/crates/trinity-stack/known-stacks.json);
2. adds its row to the table above;
3. for a signed stack, writes its copies in `stacks/` with `scripts/update-stacks.sh`. An unsigned stack is only the
   JSON entry and the table row.

Users get the stack with the next release of `@parity/trinity-stack` and the crate.

A host build that sets `reservedValues.allowed`, as a Polkadot build does, pins its stack to the copy it bundles: the
chain roles and their genesis hashes, `assets`, `contracts`, `dotns`, `coinage` and the `sha256` of the relay's
`lightSpec`. Refresh the stack's copy in `stacks/` with `scripts/update-stacks.sh` before such a build is released, so
the copy it bundles has the published values of those fields. Run the script with a `truapi-host` built from the same
tree, `TRUAPI_HOST=target/debug/truapi-host` after `cargo build -p truapi-host-cli`. The script and the daily
`update-chain-specs.yml` otherwise run the installed release, which can be older than the tree and keep a copy whose
only change is to a pinned field.

A build whose copy predates a change to one of those fields refuses each newer descriptor that carries the change and
runs the values it bundles, also on a device that stored the newer copy under an earlier build. For the same reason, the
operator of a pinned stack cannot change those fields without stranding the builds already shipped: each one refuses the
new descriptor and stays on the copy it holds until a new build replaces it. A pinned stack cannot follow a wipe of its
chains either, since the new genesis hashes are pinned fields: a build that sets `allowed` for it, such as dotli for
pcf-devnet, stays on the wiped chains until a host build that bundles the new descriptor is deployed.

## What else is here

- `Document::parse` and `open` read a document as served and apply a pointer's rules: a pointer with a key accepts only
  an envelope that verifies under strict Ed25519.
- `Pointer` encodes and decodes the links a QR code carries, and `same_stack` and `stack_id` say which stack it is.
- `sign` wraps a descriptor in a signed envelope, after checking it.
- `render` turns a model, a descriptor without `$v`, `sequence` and `generation`, into a checked descriptor, and returns
  the `PublishState` to keep: `sequence` stays while the content is unchanged and rises when it changes. `canonical` and
  `content_hash` are the rendering and hash it uses.
- The `js` feature builds the wasm-bindgen surface `@parity/trinity-stack` ships, with TypeScript types for the crate's
  values.
- `light_spec_genesis` and `Chain::check_light_spec` check a light spec against its chain's genesis hash.
- `StackReader` refreshes each stack's copy by a `Refresh` policy: it serves the copy it has, decides when to fetch, and
  takes the answer, so a descriptor is never a live dependency.
- `Descriptor::web_url` gives the URL a dotNS name opens at on the stack's `web-gateway`.
- `Chain::genesis`, `Descriptor::service`, `Descriptor::dotns_address` and `Descriptor::contract_address` return a
  chain's genesis hash, a service by kind, a dotNS address and a named contract's address as bytes. They decode hex that validation already checked, and panic only on a descriptor that
  skipped validation.

Every error is one rule of the RFC, and `Error::code` is its stable name.

## Releasing a change

`truapi-host stack` links this crate and ships with `@parity/truapi`, and `@parity/trinity-stack` ships it as wasm. A
change here reaches both only if its changeset names both packages. A release of `@parity/trinity-stack` also publishes
the crate to crates.io with the package's version.

## Conformance

[`vectors/cases.json`](https://github.com/paritytech/trinity-user-agents/blob/main/rust/crates/trinity-stack/vectors/cases.json)
holds a case for every rule, generated by `tests/vectors.rs` and run by every implementation. The vectors and their
generator are in the repository, not in the crates.io package. `vectors/refresh.json` holds the reference
`StackReader`'s refresh cases, which fix choices the RFC leaves to each reader and are informative. After a deliberate
change, rerun with `TRINITY_STACK_WRITE_VECTORS=1 cargo test -p trinity-stack --test vectors`.

Each case breaks one rule, so the expected code does not depend on the order a reader checks them in. The one exception
is a non-canonical key or `R`: no signature can verify under a point nobody knows the discrete log of, so those cases
also fail the equation, and a reader passes them by checking encodings before the equation.
`schema/trinity-stack.v1.schema.json` describes the descriptor's shape.
