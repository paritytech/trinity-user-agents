---
"@parity/truapi-host": major
---

The core serves products under the stack's own TLD. A stack may use any TLD that is a DNS label except `li` and
`localhost`, and `dot` only in a build that allows Polkadot's reserved values, so a fork with its own TLD can run
products. A host on `paseo` no longer runs `.testnet` products, and a host on `testnet` no longer runs `.paseo` ones.
`productRuntime` throws for an id the stack does not serve, and a call that names a product it does not serve answers
`MalformedFrame`.

A name can still be written under `dot` on any stack. On a stack with another TLD it names the stack's own product,
because the stack's dotNS registry resolves it: on a `paseo` host `game.dot` is the product `game.paseo`, with that
product's accounts, entropy, ring-VRF aliases, storage and everything else the core keys by the product id. The core
reads a name this way in a call or a paired host's request that names a product, and in a link, share host or Pocket
deeplink, and it answers with the id it serves. The wallet and a pairing host derive nothing under a `dot` id unless
their stack's TLD is `dot`.

A host passes the core ids its stack serves, so the two never hold different ids for one product. `productRuntime`
and `claimWorker` refuse `game.dot` on a `paseo` stack, and the error names the id to use. A runtime's
`parseProductId` gives that id for a name: it answers the id the host's stack serves the name under and rejects a name
the stack does not serve. The wasm module's own `parseProductId` checks syntax only and accepts any TLD. A product that
an earlier release ran as `name.dot` on such a stack runs as `name.<tld>`, with new accounts and none of what was
stored under the `dot` id. `clearProductState` and the permission status calls take an id as written, so a host can
still clear what was kept under the old one.

The test host follows the same rule. Its test stack is on `paseo` and its default product is `mock.paseo`. The
Playwright fixture, `createMockClient`, `productAccountAddress`, `getProductAccountAddress` and `seedProductStorage`
read `tx-demo.dot` as `tx-demo.paseo`, and `productAccountAddress` takes a `testStack` for a host on another one. A
suite that funded the account of a `.dot` product funds the new address once.

A trusted label is trusted only under an id the stack serves, in permission prompts and in the stored statuses a host
reads.

A signing host refuses a paired host's request when its stack does not serve the caller or a product the request names.
A pairing host on another stack, such as dotli on previewnet paired with a CLI wallet on paseo, gets that error on its
first relayed request for one of its own `.testnet` products.

`hasTrustedRemotePermissions` is no longer a free function of the wasm module. The wasm pairing and signing host
runtimes answer it for their stack, but `@parity/truapi-host` runs them in a Worker and does not expose it; a browser
host reads `permissionAuthorizationStatus` instead. Native hosts open the bundled stack with `Stack.openBundled(stack)`
and call `parseNavigate`, `productShareHosts`, `parseProductShareHost` and `hasTrustedRemotePermissions` on it.
