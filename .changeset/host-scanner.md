---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add the `scanner` service. `scan` asks the host to open its own QR and barcode viewfinder and returns the scanned code's text and format. No host serves it yet, so the runtime answers `Unsupported`.
