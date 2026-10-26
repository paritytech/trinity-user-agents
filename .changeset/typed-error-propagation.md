---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Runtime, CLI, and generated WASM host callbacks use shared error conversions while retaining protocol error variants and JavaScript rejection values. CLI storage and session failures retain nested causes, and allowance exhaustion is recognized through the error chain.
