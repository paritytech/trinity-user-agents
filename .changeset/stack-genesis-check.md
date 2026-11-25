---
"@parity/truapi-host": major
"@parity/truapi": patch
---

After a chain is wiped, the same endpoint serves a new chain. The core now checks for that: it asks every chain
connection for its genesis hash with `chainSpec_v1_genesisHash` before using it, including a product's connection to a
chain outside the stack. If the hash isn't the one the connection was opened for, the core drops the connection and
calls to that chain fail. A chain of the stack then shows up in the new `StackStatus.mismatches`.

Breaking: a chain whose node doesn't answer `chainSpec_v1_genesisHash` can't be used, whether or not it is in the stack.
A build with an old descriptor can't reach a wiped chain until it has the new descriptor. Pass `fetchStackDescriptor` so
the core can get it. While a chain is listed the core asks every five minutes instead of every hour. A host that only
passes `stackChanged` still hears when `mismatches` changes.

On a pairing host, `createTransaction` also fails with `ChainUnavailable` rather than `Unknown` when the node behind the
People connection that carries the request to the paired wallet hangs up or goes silent when asked for its genesis hash.
So does a dial the chain provider reports as a transport or handshake failure, as `truapi-provider` does. A
`chainConnect` or `chain_connect` callback that fails or returns no connection is still `Unknown`. A halted People chain
already gave `ChainUnavailable`. A v0.1 caller still gets `Unknown`.

An unreachable chain isn't listed, and a runtime built from `testStack` isn't checked.
