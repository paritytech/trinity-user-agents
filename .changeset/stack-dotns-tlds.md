---
"@parity/truapi-host": major
---

The core accepts product ids under `dot` and under the stack's own TLD. A stack may use any TLD that is a DNS label,
and `dot` only in a build that allows Polkadot's reserved values, so a fork with its own TLD can run products. A host on `paseo` no longer runs `.testnet` products, and a host on `testnet`
no longer runs `.paseo` ones. `productRuntime` throws for a product under any other TLD, and a call that names one
answers `MalformedFrame`.

`parseProductId` checks syntax only and accepts any TLD. `hasTrustedRemotePermissions` takes the stack's TLD as its
second argument, and so do the native `parseNavigate`, `productShareHosts`, `parseProductShareHost` and
`hasTrustedRemotePermissions`.
