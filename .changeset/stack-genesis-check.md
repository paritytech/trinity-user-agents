---
"@parity/truapi-host": minor
"@parity/truapi": patch
---

A runtime built on a bundled stack asks every chain connection it opens for the chain's genesis hash before anything
else uses the connection. A chain that gives a different hash from the descriptor's, as after it restarted from genesis,
gets no connection: calls to it fail and `StackStatus` lists it in the new `mismatches`. While a chain is listed, the
core fetches the descriptor every five minutes instead of every hour. A chain that cannot be reached stays unavailable.
A runtime built from `testStack` is not checked.
