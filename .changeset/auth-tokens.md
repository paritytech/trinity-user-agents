---
"@parity/truapi": minor
---

Products can keep the auth tokens they get from their own services with the host through the new `authTokens` API (`store`, `get`, `remove`). The host keeps each product's tokens in secure storage, apart from every other product's, and drops them on sign-out.
