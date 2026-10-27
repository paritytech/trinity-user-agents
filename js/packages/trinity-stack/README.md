# @parity/trinity-stack

Reads, verifies, signs and publishes Trinity stack descriptors, as
[RFC: Trinity stack descriptor](https://github.com/paritytech/trinity-user-agents/blob/main/docs/rfcs/trinity-stack-descriptor.md)
defines them. Every rule a reader applies runs in the
[`trinity-stack`](https://github.com/paritytech/trinity-user-agents/tree/main/rust/crates/trinity-stack) Rust crate,
compiled to WebAssembly, so this package and the host core accept and refuse the same descriptors. Its `/dotns` entry
reads a stack's dotNS contracts from the stack's Asset Hub.

## Reading a stack

```ts
import { decodeLink, memoryStore, read } from "@parity/trinity-stack";

const pointer = decodeLink("polkadotapp://stack?key=0x3b6a…&url=https%3A%2F%2F…");
const store = memoryStore(); // or your own: get/set bytes by stack
const stack = await read(pointer, { store });

stack.descriptor.chains; // checked against the pointer's key
stack.status; // updated | unchanged | conflict | rollback | unreachable | refused
stack.newGeneration; // the chains restarted: drop cached chain data
stack.stale; // past validUntil
```

`read` fetches and stores; `accept(pointer, stored, fetched, now)` is the same decision with no I/O. A reader with no
store gets no rollback protection. Errors from a rule of the RFC carry a `code`, such as `bad-signature`, which
`isTrinityStackError` recognises.

## Reading a stack's dotNS contracts

```ts
import { resolveDotns } from "@parity/trinity-stack/dotns";

const dotns = await resolveDotns(pointer, stack.descriptor, { store });
dotns?.addresses.registrar; // undefined dotns: the stack's Asset Hub has no dotNS gateway
```

Descriptors leave dotNS out, so `resolveDotns` reads it from the stack's Asset Hub: the gateway's dispatcher from
storage over the chain's RPC, then the protocol registry through its `eth-rpc`. With a store, it keeps the result under
the descriptor's `generation` and asks the chain again after `maxAgeMs`, an hour by default. If that refresh fails, it
returns the kept copy. A reader with its own client, such as a light client, passes a `transport`.

## Publishing a stack

```ts
import { encodeLink, publicKey, publish } from "@parity/trinity-stack";
import { writeFile } from "node:fs/promises";

const url = "https://previewnet.example/.well-known/trinity-stack.json";
const { document, state } = await publish(model, { generation, secret, state: previous });
await writeFile("site/.well-known/trinity-stack.json", document); // deploy site/ so `url` serves it
const link = encodeLink({ url, key: publicKey(secret) }); // what the QR code carries
```

`publish` does no I/O: it returns the bytes, and the operator serves them at the URL the link names, usually
`/.well-known/trinity-stack.json` on the stack's domain. `model` is a descriptor without `$v`, `sequence` and
`generation`. `publish` keeps `sequence` while nothing changes and raises it otherwise, never below the time in seconds.
Keep `state` and `secret` somewhere a wipe does not delete: without `state`, two publishes in the same second get the
same `sequence`, and the key in the link is the stack's identity. Any QR library can render `link`.

## Conformance

The crate's [vectors](https://github.com/paritytech/trinity-user-agents/tree/main/rust/crates/trinity-stack/vectors)
cover every rule of the RFC, and the
[JSON Schema](https://github.com/paritytech/trinity-user-agents/blob/main/rust/crates/trinity-stack/schema/trinity-stack.v1.schema.json)
covers the shape. This package's tests run the same vectors against the WebAssembly build.

## Building

`npm run build` builds the crate with `wasm-pack` into `wasm/` and the TypeScript into `dist/`; `npm test` runs the bun
tests.
