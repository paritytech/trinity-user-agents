---
"@parity/truapi-host": patch
---

Keep each product's chain follows and transaction operations to itself: a product resolves only its own follow ids, cannot stop another product's transaction by its operation id, and releasing a product drops both.
