---
"@parity/truapi-host": major
---

The core serves products under the stack's own TLD. A stack may use any TLD that is a DNS label except `li` and
`localhost`, and `dot` only in a build that allows Polkadot's reserved values, so a fork with its own TLD can run
products. A host on `paseo` no longer runs `.testnet` products, and a host on `testnet` no longer runs `.paseo` ones.
`productRuntime` throws for a product under any other TLD, and a call that names one answers `MalformedFrame`.

A name can still be written under `dot` on any stack. On a stack with another TLD the core reads it as the same name
under the stack's TLD, because the stack's own dotNS registry resolves it: on a `paseo` host `game.dot` is the product
`game.paseo`. This holds for the product a host asks the core to run, a product a call or a paired host's request
names, a link, share host or Pocket deeplink, and the product ids a host passes to the runtime. The product's accounts,
entropy, ring-VRF aliases, storage and permissions are those of `game.paseo`, and the core answers and calls back with
that id. A product that ran as `name.dot` on such a host therefore gets new accounts and starts with empty storage and
no recorded permissions. No key derives under a `dot` id except on a stack whose TLD is `dot`.

The test host follows the same rule. Its test stack is on `paseo`, so `productAccountAddress`,
`getProductAccountAddress` and `seedProductStorage` read `tx-demo.dot` as `tx-demo.paseo`, and `productAccountAddress`
takes a `testStack` for a host on another one. A suite that funded the account of a `.dot` product funds the new address
once.

A trusted label is trusted only under `dot` or the stack's own TLD, in permission prompts and in the stored statuses a
host reads.

A signing host refuses a paired host's request when its stack does not serve the caller or a product the request names.
A pairing host on another stack, such as dotli on previewnet paired with a CLI wallet on paseo, gets that error on its
first relayed request for one of its own `.testnet` products.

`parseProductId` checks syntax only and accepts any TLD. `hasTrustedRemotePermissions` is no longer a free function of
the wasm module. The wasm pairing and signing host runtimes answer it for their stack, but `@parity/truapi-host` runs
them in a Worker and does not expose it; a browser host reads `permissionAuthorizationStatus` instead. Native hosts open
the bundled stack with `Stack.openBundled(stack)` and call `parseNavigate`, `productShareHosts`, `parseProductShareHost`
and `hasTrustedRemotePermissions` on it.
