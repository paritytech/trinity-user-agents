---
"@parity/truapi": patch
---

The `truapi-host` signing host runs funding providers' workers on demand: `TRUAPI_FUNDING_PROVIDERS` takes `<product>=<worker script>` entries, and the host launches a provider's worker script as its own process when the core asks for that worker and stops it when the core lets it go. The overlay asks for quotes on the core's default rail, token and network rather than a fixed USDT on Polkadot, and the providers it lists are bundled with their manifest.
