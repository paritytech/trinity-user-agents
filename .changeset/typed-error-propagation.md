---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Runtime, CLI, and generated WASM host callbacks use shared error conversions while retaining protocol error variants and JavaScript rejection values. CLI storage failures include the operation and filesystem cause, and allowance exhaustion is recognized through error context.
