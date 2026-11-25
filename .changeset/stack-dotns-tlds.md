---
"@parity/truapi-host": major
---

The core accepts product ids under `dot` and under the stack's own TLD. A stack may use any TLD that is a DNS label
except `li` and `localhost`, and `dot` only in a build that allows Polkadot's reserved values, so a fork with its own
TLD can run products. A host on `paseo` no longer runs `.testnet` products, and a host on `testnet` no longer runs
`.paseo` ones. `productRuntime` throws for a product under any other TLD, and a call that names one answers
`MalformedFrame`. A trusted label is trusted only under a TLD the stack serves, in permission prompts and in the stored
statuses a host reads.

A signing host refuses a paired host's request when its caller, or a product it names, is under a TLD the signing host's
stack does not serve. A pairing host on another stack, such as dotli on previewnet paired with a CLI wallet on paseo,
gets that error on its first relayed request for one of its own `.testnet` products.

`parseProductId` checks syntax only and accepts any TLD. `hasTrustedRemotePermissions` is no longer a free function of
the wasm module. The wasm pairing and signing host runtimes answer it for their stack, but `@parity/truapi-host` runs
them in a Worker and does not expose it; a browser host reads `permissionAuthorizationStatus` instead. Native hosts open
the bundled stack with `Stack.openBundled(stack)` and call `parseNavigate`, `productShareHosts`, `parseProductShareHost`
and `hasTrustedRemotePermissions` on it.
