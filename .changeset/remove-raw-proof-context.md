---
"@parity/truapi": major
"@parity/truapi-host": major
---

`development_createAccountProof` is not exported, and signing and pairing hosts derive every ring-VRF proof context from
its product id. A `raw:` product id has no special meaning: its context is derived like any other product's, so a proof
can no longer be bound to 32 caller-chosen bytes.
