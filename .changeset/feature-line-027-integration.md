---
"@parity/truapi": major
"@parity/truapi-host": major
---

Integrate canonical 0.27 product/session ownership with feature-line Chat v2, native media, profiles, JAM and Coinage. Hosts must regenerate bindings and use a matching runtime binary. Desktop hosts can select explicit account/signature consent and inspect or recover existing product resources without retrying uncertain allocations. Signing cancellation, automatic-upload consent and product-scoped revocation remain enforced.

This is a feature-line compatibility cutover, not a drop-in replacement for every standalone canonical 0.27 host: account-payment, balance and payment/top-up status APIs are explicitly unsupported; incoming top-ups and Chat custody remain implemented. Existing feature-line storage tags are retained. Canonical-only Funding stores require an explicit migration to the integration's noncolliding keys before reuse. See the runtime integration boundary in `rust/crates/truapi/RUNTIME.md`; do not infer a release or physical-device qualification from this changeset.

Runtime packaging retains the combined MIT/AGPL license notices. Vendored-runtime tests do not assume the upstream workspace's directory layout or Cargo profile text.

Canonical CLI/mobile builds patch `ruzstd` 0.8.3 with upstream RUSTSEC-2026-0344 fix `d3907c960ca4a1aa44cc38340137c5679dd98f5a` plus bounds on regenerated and trailing literals, retaining smoldot 4 and the independent PVM pin. `rust/vendor/ruzstd/UPSTREAM.json` and `PATCHES.json` distinguish original source, the upstream patch and the additional literal-path fixes. This is a source-specific backport, not a claim that registry `ruzstd` 0.8.3 is patched: downstream Cargo roots must explicitly select this crate from the same immutable core revision as their provider and verify its provenance before treating that advisory as remediated. The canonical lock selects rustls 0.23.46, rustls-webpki 0.103.16, webpki-roots 1.0.10, webpki-root-certs 1.0.10 and Tokio 1.53.3, and replaces yanked clipboard-win 5.4.1 with 5.4.3. Security advisory checks remain required.
