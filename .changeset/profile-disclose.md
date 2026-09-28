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
