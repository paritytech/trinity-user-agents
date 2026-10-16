---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Keep capability logic on latest protocol payloads and return responses, subscription items, and domain errors in the requested version. Local storage reads preserve V1 and V2 at the handler boundary.
