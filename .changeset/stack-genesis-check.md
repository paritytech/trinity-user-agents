---
"@parity/truapi-host": minor
"@parity/truapi": patch
---

For a stack loaded from its descriptor, the core asks every chain connection it opens for the chain's genesis hash before
anything else uses the connection. A chain that gives another hash than the descriptor names, as one does once it
restarted from genesis, gets no connection: calls to it fail, the core fetches the descriptor again at once, and
`StackStatus` lists the chain in the new `mismatches`. A chain that cannot be reached stays unavailable. Stacks built
from `testStack` or `Stack::new` are not checked.
