---
"@parity/truapi": major
"@parity/truapi-host": major
---

Integrate canonical 0.27 product/session ownership with feature-line Chat v2, native media, profiles, JAM and Coinage. Hosts must regenerate bindings and use a matching runtime binary. Desktop hosts can select explicit account/signature consent and inspect or recover existing product resources without retrying uncertain allocations. Signing cancellation, automatic-upload consent and product-scoped revocation remain enforced.

This is a feature-line compatibility cutover, not a drop-in replacement for every standalone canonical 0.27 host: account-payment, balance and payment/top-up status APIs are explicitly unsupported; incoming top-ups and Chat custody remain implemented. Existing feature-line storage tags are retained. Canonical-only Funding stores require an explicit migration to the integration's noncolliding keys before reuse. See the runtime integration boundary in `rust/crates/truapi/RUNTIME.md`; do not infer a release or physical-device qualification from this changeset.
