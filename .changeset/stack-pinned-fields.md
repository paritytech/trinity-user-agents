---
"@parity/truapi-host": minor
"@parity/truapi": patch
---

A stack its build allows reserved values is pinned to the descriptor the build bundles. With `reservedValues.allowed`
set, the core refuses a fetched copy that gives a chain role another genesis hash, adds or drops a chain role, or
changes `assets`, `contracts`, `dotns`, `coinage` or the `sha256` of the relay's `lightSpec`. `lastRefresh` is then
`refused` and the core keeps the copy it holds. A stored copy that differs in one of them is ignored, and the core
starts from the bundled copy. Only a new build changes one of them. A fresher relay checkpoint is such a change, and so
is a Polkadot fork made again from a later block, because both change the relay's `lightSpec`. The pins cover what the
descriptor names, not what a host reads from the chains.

`truapi-host stack bundle --skip-same-projection` also compares every field the core pins, so a published copy that
changes only one of them is still written.
