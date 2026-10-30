# @parity/trinity-stack

Reads, verifies, signs and publishes Trinity stack descriptors, as
[RFC: Trinity stack descriptor](https://github.com/paritytech/trinity-user-agents/blob/main/docs/rfcs/trinity-stack-descriptor.md)
defines them. Every rule a reader applies to a descriptor runs in the
[`trinity-stack`](https://github.com/paritytech/trinity-user-agents/tree/main/rust/crates/trinity-stack) Rust crate,
compiled to WebAssembly, so this package and the host core accept and refuse the same descriptors.

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

## Running from build copies

A project ships the stacks it uses, so it keeps working when a descriptor's server is down:

```sh
npx trinity-stack add previewnet "polkadotapp://stack?key=0x4d43…&url=…"
npx trinity-stack update   # exits 2 when it wrote something, so a scheduled job can open a pull request
```

They write `.trinity/`, committed with the project: each stack's signed copy and an `index.js` that bundles them. A
stack that fails to update keeps its copy, and `update` exits 1. At run time a reader serves those copies and refreshes
them by the policy the project picks:

```ts
import { createStackReader, service } from "@parity/trinity-stack/reader";
import { stacks } from "../.trinity/index.js";

const reader = createStackReader({ bundled: stacks, refresh: "once" }); // "never" | "once" | { maxAgeMs }
const { descriptor } = await reader.get(stacks.previewnet, {
  require: { chains: ["AssetHub"], services: ["eth-rpc"] },
});
const ethRpc = service(descriptor, "eth-rpc");
reader.onChange(({ newGeneration }) => {
  /* reconnect, and drop cached chain data on a new generation */
});
```

It never fetches per request. A fetch that fails leaves the copy it has in use, and a `store` of the project's own lets
a restart start from the last copy fetched. A conditional fetch the server's CORS blocks is retried at once without
`If-None-Match`. Every other decision is the crate's `StackReader`, so a host that drives the same reader behaves the
same way.

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
