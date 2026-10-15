---
"@parity/truapi-host": patch
---

A signing host's wallet lives in one `WalletAccountHolder`, the only reader of the root entropy, and it checks that the wallet activation a call began under is still installed each time it uses the keys. A wallet lock or switch now fails a VRF signature confirmed under the previous wallet, a transaction whose chain metadata was still loading, an alias or proof still preparing its ring, and a pairing answer still reading the device key, instead of completing them with the new wallet's keys or the old one's. A lock during a resource allocation fails the request instead of marking the remaining resources unavailable.
