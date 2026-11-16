---
"@parity/truapi": patch
---

`sign_vrf` with another product's account follows the rules of the other signing methods: it needs that product's `context` grant, and answers `Rejected` without a prompt otherwise. A first-party product holding the grant signs without a prompt.
