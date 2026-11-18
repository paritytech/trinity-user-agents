---
"@parity/truapi-host": major
---

`SessionUiInfo.deviceEncPublicKey` and `getDeviceEncryptionKey()` name this device's own X25519 encryption key, as its account holder keeps it. A signing host serves its install-wide key, the one it answers pairing proposals with. A pairing host serves the key it advertised in its pairing proposal, which the wallet registers with the device's statement account and announces to the user's contacts, so a host running its own chat or device sync reaches the device the wallet registered. On a pairing host both are `undefined` before pairing and change with every login, and `getDeviceEncryptionKey()` resolves to `Bytes32 | undefined`. The paired wallet device's key is `SessionUiInfo.peerDeviceEncPublicKey`.
