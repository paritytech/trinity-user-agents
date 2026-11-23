---
"@parity/truapi-host": major
"@parity/truapi": patch
---

The mock host reports a chain as supported only when it serves it: `onlyServedChains` (Rust `MockConfig::only_served_chains`) defaults to `true`, so `featureSupported` answers from `servedChains`.
The test fixture fills `servedChains` from `networks` and `chain`. A suite that proxies a chain through `mock.chainProxies` alone lists its genesis in `mock.servedChains`, and `onlyServedChains: false` reports every chain as supported.
