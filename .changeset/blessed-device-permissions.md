---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Blessed products (`peopl`, `dim2` and `stash`) get device permissions, such as notifications, without a prompt: the core does not call `device_permission` for them. They still need the OS grant. Stored device decisions do not apply to them, so per-product device toggles in the host have no effect.
