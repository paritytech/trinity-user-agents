---
"@parity/truapi-host": major
---

A host build can add to the core's reserved values. `stack.reservedValues` replaces `stack.allowsReservedValues`, and
the test host's `testStack.reservedValues` replaces `testStack.allowsReservedValues`. Both take `allowed`, whether the
stack may name a reserved value, and the `extraGenesisHashes` and `extraTlds` the build reserves on top of Polkadot's
genesis hashes and the `dot` TLD. A build cannot remove a value from the core's list. The core refuses a
`reservedValues` with a field it does not have, and an `extraTlds` entry that is not one lowercase label without the
leading dot.
