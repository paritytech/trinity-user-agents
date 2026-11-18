---
"@parity/truapi-host": major
---

`getDeviceEncryptionKey()` and `SessionUiInfo.deviceEncPublicKey` name this device's own encryption key. On a paired host it is the key from the pairing QR, which the wallet registered: `undefined` before pairing, and new with every login. The paired wallet device's key is `SessionUiInfo.peerDeviceEncPublicKey`.
