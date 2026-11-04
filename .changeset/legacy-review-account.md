---
"@parity/truapi-host": major
---

Legacy-account sign payload, sign raw and create transaction reviews carry the account the core resolved the signer
to: the wallet's identity account or the product's account at index 0. Hosts render the signer from it instead of
resolving the request's address themselves.
