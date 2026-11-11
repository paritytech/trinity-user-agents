---
"@parity/truapi-host": minor
---

Native hosts load contact avatars with `loadImage` and a `Username` reference. The core caches dotNS profile records for one hour, with an optional forced refresh, and loads avatars through the shared Bulletin image loader. The result carries verified bytes, their format and the CID content key. A missing avatar is `ImageError::Unavailable`. The Rust WASM resolver reads the chain without a record cache.
