---
"@parity/truapi": minor
---

The core reads Worker manifests v1 and v2 from dotNS, with the `includes.funding` configuration, cached for a day. Hosts list their funding providers with `set_funding_providers`, each with a Worker manifest snapshot, and read `funding_candidates(intent)`; `select_funding_provider` accepts only a candidate serving the session's direction.
