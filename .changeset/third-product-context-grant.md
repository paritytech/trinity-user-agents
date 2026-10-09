---
"@parity/truapi-host": patch
---

A product granted another product's ring-VRF key may now prove in, and read its alias in, the context of a third product
whose manifest grants it `context` (or `all`). The third product must be on the key owner's network, and a refusal the
user stored for that pair still applies. A third product that grants nothing, and the `raw:` development context, stay
refused with `NotAllowlisted`.
