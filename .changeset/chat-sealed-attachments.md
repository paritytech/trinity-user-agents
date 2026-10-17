---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add sealed chat attachments: a room that declares `sealed_attachments` gets a user attachment above the inline bound as `ChatResource::Sealed { parts, key, nonce, size, digest }`. The core encrypts the bytes with XChaCha20-Poly1305 under a new key, stores the ciphertext in Bulletin parts that the product's allowance pays for (at most 32 MiB), and `HostChatCapabilitiesResponse` gains `sealed_attachment_max_bytes`. A native host seals with `NativeTrUApiHostRuntime.chat_seal_attachment`.
