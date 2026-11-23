---
"@parity/truapi": minor
---

Adds `ringCollectionId(name)`, which encodes a ring collection name such as `"pop:polkadot.network/people-lite"` as the 32-byte `CollectionId` junction of a `RingLocation`, padded with spaces the way the People chain stores it.
