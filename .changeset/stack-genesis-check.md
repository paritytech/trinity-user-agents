---
"@parity/truapi-host": major
"@parity/truapi": patch
---

A runtime built on a bundled stack asks every chain connection it opens for the chain's genesis hash before anything
else uses the connection. A chain that gives a different hash from the descriptor's, as after it restarted from genesis,
gets no connection: calls to it fail and `StackStatus` lists it in the new `mismatches`. While a chain is listed, a host
with `fetchStackDescriptor` is asked for the descriptor every five minutes instead of every hour. A host that passes
`stackChanged` without it is told when `mismatches` changes. A chain that cannot be reached stays unavailable. A runtime
built from `testStack` is not checked.
