---
"@parity/truapi-host": major
---

The core accepts product ids under `dot` and under the stack's own TLD. A stack may use any TLD that is a DNS label,
and `dot` only in a build that allows Polkadot's reserved values, so a fork with its own TLD can run products. A host on
`paseo` no longer runs `.testnet` products, and a host on `testnet` no longer runs `.paseo` ones. `productRuntime` throws
for a product under any other TLD, and a call that names one answers `MalformedFrame`. A trusted label is trusted only
under a TLD the stack serves, in permission prompts and in the stored statuses a host reads.

`parseProductId` checks syntax only and accepts any TLD. `hasTrustedRemotePermissions(productId)` is a method of the
pairing and signing host runtimes, answered for the runtime's stack. Native hosts open the bundled stack with
`Stack.openBundled(stack)` and call `parseNavigate`, `productShareHosts`, `parseProductShareHost` and
`hasTrustedRemotePermissions` on it.
