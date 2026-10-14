---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Allow `polkadotapp://` links through `system.navigateTo`, preserving payment parameters and requiring `OpenUrl` permission. Reject external `polkadot:` and `dot:` links.
