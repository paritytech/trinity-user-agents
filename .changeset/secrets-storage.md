---
"@parity/truapi": minor
---

Products can keep small secrets, such as the tokens they get from their own services, through the new `secretsStorage` API (`read`, `write`, `clear`). The product names a key and the core keeps the bytes in the host's encrypted storage under an id it derives from the signed-in account, the product and that key, so no product or account reads another's, and drops them when the session ends or the account changes. The browser core answers `Unsupported`.
