---
"@parity/truapi-host": major
---

The core refuses a stack without an Asset Hub chain, as it does one without People or Bulletin. `TestStack.assetHub` is
required, and the native `PublicProfileError` has no `AssetHubNotConfigured` case.
