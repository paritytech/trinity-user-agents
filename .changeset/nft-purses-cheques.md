---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Native hosts serve `NftPurse.createReceiveKey` and `NftPurse.createCheque`. `createReceiveKey` makes a one-time X25519 receive key into any purse, with no grant; its secret is kept in the host's encrypted storage through the new `encryptedStorageRead` and `encryptedStorageWrite` host callbacks, which default to a rejection. `createCheque` seals the secret of each item to the receive key or a static X25519 key and returns the cheque for the product to deliver; the items are spent from then on. `encryptedSecrets` is the ephemeral X25519 key, the 12-byte nonce, and the ChaCha20-Poly1305 ciphertext (HKDF-SHA256 key) of the SCALE-encoded 64-byte sr25519 secrets in `items` order. An ephemeral cheque id is `blake2_256("nft-purse/cheque-id" ++ receiveKey)`. `NftPurseCreateChequeError` gains `NoItems` and `OperationExists`, the latter for a second cheque with the same id from the same product. Handing off items of a purse the product does not own asks the user through `NftPurseDrain`, whose destination gains `Cheque`. The browser core answers both calls `Unsupported`.
