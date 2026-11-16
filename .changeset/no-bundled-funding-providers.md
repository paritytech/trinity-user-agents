---
"@parity/truapi": major
---

Native hosts offer only published funding providers: `set_funding_provider_config` takes the published names alone, and
`bundled_funding_providers` is gone. Each name carries no TLD (`fundtest-provider`, not `fundtest-provider.paseo`): the
core reads it on the network it is connected to, so one remote configuration serves every network, and a name that
names a TLD is skipped. `FundingProviderEntry`, `FundingProviderApp` and `FundingProviderOrigin` stay on the Rust
runtime, where `set_funding_providers` takes them.
