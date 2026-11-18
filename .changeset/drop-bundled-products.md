---
"@parity/truapi": major
---

Native hosts run only published and dev-server products. `bundled_products` and `BundleEntry` are gone, `ProductSource`
has no `Bundled` case, and `product_source` takes no `bundle`. Archive serving stays: `archive_file` takes an
`ArchiveReader`, which only answers `exists`. On the Rust runtime, `FundingProviderEntry` carries only `product_id` and
`worker_manifest`; `FundingProviderOrigin` and `FundingProviderApp` are gone, and whether a provider has an App is
always read from dotNS.
