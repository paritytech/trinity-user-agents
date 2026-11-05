---
"@parity/truapi-host": major
---

Legacy-account sign payload, sign raw and create transaction reviews carry the account the core resolved the signer
to: the wallet's identity account or the product's account at index 0, so a host knows the signer without resolving
the request's address itself. Android's signing sheet uses it.
