---
"@parity/truapi": minor
---

The core reads a funding provider's Worker manifest from dotNS and caches it for a day like the root manifest, taking its `includes.funding` configuration with unrecognised values left out. Funding routes are optional: a configuration without a usable route still serves Funding.
