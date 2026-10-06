---
"@parity/truapi": patch
---

Funding accounts are derived with getcash's scheme under the reserved `fund.<network suffix>` product: that product's entropy for the account's getcash label, taken as a mini secret. The accounts are the funding product's, not getcash's own, so burners getcash already made under its product id are not among them. No product under `fund.<network suffix>` can derive entropy.
