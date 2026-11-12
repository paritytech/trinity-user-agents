---
"@parity/truapi": major
---

Native hosts offer only published funding providers: `set_funding_provider_config` takes the published names alone, and
`bundled_funding_providers` is gone. `FundingProviderEntry`, `FundingProviderApp` and `FundingProviderOrigin` stay on the
Rust runtime, where `set_funding_providers` takes them.
