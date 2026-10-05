---
"@parity/truapi": patch
---

Funding accounts are derived as getcash derives its burners: the `fund.<network suffix>` product's entropy for the account's label, taken as a mini secret. No product under that name can derive entropy.
