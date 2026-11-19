---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Adds `ImageSource::Inline`, an image carried in the renderer tree itself, so a product can draw a picture it makes at run time. It is PNG, JPEG or WebP and at most 64 KiB, and in JSON its bytes are a `0x` hex string. A host built before this release cannot draw a tree that uses it.
