---
"@parity/truapi-host": minor
---

Native hosts can provide encrypted storage for the secrets the core keeps through the new `encryptedStorageRead`, `encryptedStorageWrite` and `encryptedStorageDelete` host callbacks, which default to a rejection. A missing key reads as absent; a store that cannot be read, or a value that no longer decrypts, is an error.
