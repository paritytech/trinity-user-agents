---
"@parity/trinity-stack": minor
"@parity/truapi-host": minor
"@parity/truapi": patch
---

The `trinity-stack` crate's `Descriptor::contract_address` returns the address of a contract the descriptor names in
`contracts`. The core's `Stack` exposes the `AccountDataStore` contract, where each seed registers its installations, as
`account_data_store`, or `None` when the stack names none. `truapi-host stack bundle --skip-same-projection` counts
`contracts` as read, so a published copy that changes only them is still written.
