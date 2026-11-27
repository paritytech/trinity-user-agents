---
"@parity/truapi": patch
---

The `truapi-host` signing host offers funding providers published on dotNS, as the apps do: `TRUAPI_FUNDING_PROVIDER_NAMES` lists them by name without a TLD, and the core reads each one's manifest from dotNS. The host runs a provider's worker on demand, as its own process when the core asks for that worker, and stops it when the core lets it go: the worker the provider publishes, or a local script `TRUAPI_FUNDING_WORKERS` names (`<product>=<worker script>`). The overlay waits for the providers' manifests to be read, and asks for quotes on the core's default rail, token and network rather than a fixed USDT on Polkadot.
