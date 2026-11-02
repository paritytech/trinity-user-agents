---
"@parity/truapi": patch
---

The `truapi-host` signing host runs funding providers' workers on demand: `TRUAPI_FUNDING_PROVIDERS` takes `<product>=<worker script>` entries, and the host launches a provider's worker script as its own process when the core asks for that worker and stops it when the core lets it go.
