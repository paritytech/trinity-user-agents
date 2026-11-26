---
"@parity/truapi-host": major
"@parity/truapi": patch
---

A stack its build allows reserved values is pinned to the descriptor the build bundles. With `reservedValues.allowed`
set, the core refuses a fetched copy that gives a chain role another genesis hash, adds or drops a chain role, or
changes `assets`, `contracts`, `dotns` or `coinage`. `lastRefresh` is then `refused` and the core keeps the copy it
holds. A stored copy that differs in one of them is ignored, and the core starts from the bundled copy. Only a new
build changes one of them. The pins cover what the descriptor names, not what a host reads from the chains.

Such a stack also runs the relay's `lightSpec` from the bundled descriptor, so a light client starts from the sync
checkpoint the build ships. A newer copy that names a fresher relay light spec is taken and its other changes are
followed, but `StackStatus.descriptor` keeps the bundled relay `lightSpec`.

A host that sets `allowed` for a stack whose operator wipes its chains, redeploys its contracts or changes its assets,
as dotli does for pcf-devnet, no longer follows those changes: it needs a new build that bundles the new descriptor
each time.

`truapi-host stack bundle --skip-same-projection` also compares every field the core pins for a stack its build allows
reserved values, on every stack, so a published copy that changes only one of them is still written.
