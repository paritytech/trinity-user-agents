---
"@parity/truapi": minor
---

The core reads Worker manifests v1 and v2 from dotNS, with the `includes.funding` configuration, cached for a day. Hosts list their funding providers with `set_funding_providers`, each with a Worker manifest snapshot that must parse, and the core offers each session the candidates serving its direction; `select_funding_provider(intent, provider, quote_id)` takes the quote the user chose the provider on, which must be one the core offered for the session.
