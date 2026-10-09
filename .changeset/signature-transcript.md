---
"@parity/truapi": minor
---

With `TRUAPI_SIGNATURES_LOG` set, a native host appends each signature it returns from a local signing (sign payload, sign raw, create transaction) to that JSON-lines file, with the signer and, for a created transaction, the complete signed transaction. Unset, nothing is written.
