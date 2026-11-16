---
"@parity/truapi-host": major
---

The core takes the dotNS TLDs it accepts from the stack: Polkadot's reserved `dot` and the stack's own `tld`. A stack
may name any TLD that is a DNS label, so a fork with its own TLD runs products. A host on `paseo` no longer runs
`.testnet` products, and one on `testnet` no longer runs `.paseo` ones. `productRuntime` throws for a product under a
TLD the stack does not serve, and a call naming such a product answers `MalformedFrame`. A trusted label is trusted only
under a TLD the stack serves, in permission prompts and in the stored statuses a host reads.

`parseProductId` checks syntax only, so it accepts a name under any TLD. `hasTrustedRemotePermissions(productId)` is a
method of the pairing and signing host runtimes, answered for the runtime's stack. Native hosts open the bundled stack
with `Stack.openBundled(stack)` and call `parseNavigate`, `productShareHosts`, `parseProductShareHost` and
`hasTrustedRemotePermissions` on it.
