---
"@parity/truapi": minor
---

The core resolves a person's public profile from the `profile` text record under their dotNS name. `resolve_public_profile(label, force)` on the native runtime walks dotNS as it does for a product manifest, decodes the record (`ProfileRecord`: typed entries, of which `avatar` and `name` are read), fetches the avatar bytes through the host's preimage backend and keeps them only if they hash to the CID, and answers Found with the bytes and the name, Absent when the name has no record, or Failed when the chain was out of reach. The decoded record is cached for an hour in core storage; the bytes are the host's to keep. The lookup is host-facing only.
