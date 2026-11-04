---
"@parity/truapi-host": minor
---

Native hosts load images with `loadImage(reference)`. The core reads a Bulletin blob by CID over its own Bulletin connection, returns it only once it hashes to the CID, passes on PNG, JPEG, GIF and WebP up to 2 MiB and 4096 pixels per side, and rejects bytes in a format other than the one the reference declares. The result carries the bytes, their format and the CID as a content key. The browser core does not expose it.
