---
"@parity/trinity-stack": patch
"@parity/truapi": patch
---

A descriptor or pointer URL that is not ASCII is now refused, since IDNA readers map some hosts differently. A host
outside ASCII must be written in its `xn--` form.
