---
"@parity/truapi": major
---

Native hosts run only published and dev-server products. `bundled_products` and `BundleEntry` are gone, `ProductSource`
has no `Bundled` case, and `product_source` takes no `bundle`. Archive serving stays, without a callback:
`archive_files` returns the `ArchiveFile`s that may answer a request, in order, and the host serves the first it has;
`BundleReader` is gone with no replacement. MIME types come from `mime_guess`, so `mjs` is `application/javascript`,
`map` `text/plain`, `xml` `text/xml`, `woff` `application/font-woff` and `otf` `application/font-sfnt`. On the Rust
runtime, `FundingProviderEntry` carries only `product_id` and `worker_manifest`; `FundingProviderOrigin` and
`FundingProviderApp` are gone, and whether a provider has an App is always read from dotNS.
