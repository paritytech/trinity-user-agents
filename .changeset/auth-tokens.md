---
"@parity/truapi": minor
---

Products can keep the auth tokens they get from their own services with the host through the new `authTokens` API (`store`, `get`, `remove`). The host keeps them in secure storage under a key the core derives from the signed-in account, the product and the service, so no product or account reads another's, and the core drops them when the session ends.
