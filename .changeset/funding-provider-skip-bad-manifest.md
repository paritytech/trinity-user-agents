---
"@parity/truapi": patch
---

A funding provider whose shipped Worker manifest does not parse, or a bundled one shipped without it, is logged and left out; `set_funding_providers` still offers the rest instead of refusing the whole list. Since nothing in either call can fail any more, `set_funding_providers` and `set_funding_provider_config` no longer return a `Result` (natively `setFundingProviderConfig` no longer throws; an existing `try` or `runCatching` around it still compiles). Bundled product archives also serve the usual web media, document and font types (mp4, webm, mp3, ogg, wav, pdf, avif, xml, csv, eot, webmanifest) with their MIME type rather than `application/octet-stream`.
