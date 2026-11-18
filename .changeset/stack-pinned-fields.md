---
"@parity/truapi-host": minor
---

A stack its build allows reserved values is pinned to the descriptor the build bundles. With `reservedValues.allowed`
set, the core refuses a fetched copy that gives a chain role another genesis hash, adds or drops a chain role, or
changes `assets`, `contracts`, `dotns`, `coinage` or the `sha256` of the relay's `lightSpec`. `lastRefresh` is then
`refused` and the core keeps the copy it holds. A stored copy that differs in one of them is ignored, and the core
starts from the bundled copy. Changing one takes a new build, which includes a fresher relay checkpoint and a Polkadot
fork made again from a later block. Every other field, `generation` included, still follows a refresh.
