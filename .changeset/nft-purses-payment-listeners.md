---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Native hosts serve `NftPurse.listenOneTimePayment`, which replaces `listenForPayment`, and the new `NftPurse.listenPaymentInbox`. A sender publishes a statement whose data is the cheque sealed to the receiver's key: the ephemeral X25519 key, the nonce, then the ChaCha20-Poly1305 ciphertext of `SCALE(version: u8 = 0, topic, cheque)`. `listenOneTimePayment` watches `blake2_256("nft-purse/payment-channel" ‖ receiveKey)` for a receive key the product made, emits that channel, then the first cheque addressed to the key, and ends; another product's key answers `ReceiveKeyNotFound`. `listenPaymentInbox` watches `blake2_256("nft-purse/payment-inbox" ‖ identityAccount ‖ blake2_256(productId) ‖ inboxDomain)` for `Static` cheques sealed to the static receive key, emits that channel, then every cheque already stored as one batch, possibly empty, then each batch of new ones. Both emit cheques already deposited again while the store holds them, and end with `Other` when the connection drops. The browser core answers both `Unsupported`.
