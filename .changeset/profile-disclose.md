---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add `profile.disclose`, `profile.retract` and `profile.presentContact`. A product discloses one opaque reference to the
user's chat contacts and may withdraw it; a product names a contact by peer identity and the host presents the reference
that contact disclosed, so no product holds a contact's reference. The first `disclose` from a product asks the user
once through `userConfirmation.confirmPermission` with a new `ProfileDisclosure` review, remembered as the
`ProfileDisclosure` permission; a refusal is `PermissionDenied`. Hosts must render that review.

This change includes the Chat relay. The host sends the disclosure to every ready Chat v2 contact as a host-private
message and keeps, per contact, the newest frame their host sent back, withdrawals included, whatever order the chat
product opens them in. Both live in wallet- and network-scoped core storage (`ProfileDisclosure`,
`ProfileReferencesReceived`). Delivery is best effort: relayed references never take outbox room from other Chat traffic
and are dropped, not re-signed, after one statement lifetime.

Add `profile.placeContactAvatars`. A chat App tells the host where it draws contacts' avatars (surface size and, per
avatar, a slot id, peer identity, square rect and clip), and the host draws the photo and mood ring of each contact who
shared a profile with it on its own layer. The core filters the placement to contacts with a current reference, hands
them with their references to the new `ProfilePlatform.placeContactAvatars(product, placed)` callback, and
redraws the remembered placement when a reference arrives or is withdrawn; it clears it when the connection goes away.
The product is answered `Ok` whoever shared; only a malformed placement (more than 64 slots, a surface side outside 1 to
16384, a non-square avatar or one outside 1 to 1024 a side, a repeated slot) is refused, and a host that cannot draw
answers `Unsupported`. A JS host that supplies a `profile` group must implement the callback; the Rust trait's default
draws nothing.
