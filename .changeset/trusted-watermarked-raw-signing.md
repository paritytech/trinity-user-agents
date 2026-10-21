---
"@parity/truapi-host": patch
---

A blessed product signs raw bytes with the user's legacy account without a prompt when the bytes are signed inside the `<Bytes>` watermark, which cannot pass for a transaction's signing payload. The deprecated unwatermarked raw signing, payload signing and transaction creation with a legacy account still ask the user, as does every legacy-account signature for a product that is not blessed.
