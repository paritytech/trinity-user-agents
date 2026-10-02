---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add `profile.disclose`, `profile.retract` and `profile.presentContact`. A product discloses one opaque reference to the
user's chat contacts and may withdraw it; a product names a contact by peer identity and the host presents the reference
that contact disclosed, so no product holds a contact's reference. The first `disclose` from a product asks the user
once through `userConfirmation.confirmPermission` with a new `ProfileDisclosure` review, remembered as the
`ProfileDisclosure` permission; a refusal is `PermissionDenied`. Hosts must render that review.

This change includes the Chat relay. In legacy `ChatApps` mode the host sends the disclosure to every ready Chat v2
contact as a host-private app-scoped message and keeps, per contact, the newest frame their host sent back, withdrawals
included, whatever order the chat product opens them in. Both live in wallet- and network-scoped core storage
(`ProfileDisclosure`, `ProfileReferencesReceived`). The host queues the disclosure when the chat product initializes or
reconciles, in the response to a Chat request in which a contact became ready, and, without delaying the call, as soon
as `disclose` or `retract` changes it while a Chat of the same wallet is open; the chat product still has to run to
submit it. Delivery is best effort: relayed references never take outbox room from other Chat traffic, and one that
lapses unacknowledged after a statement lifetime is signed again for a ready contact, at most three frames per contact
and disclosure. Every `disclose` call is a new disclosure, even with the reference already held: a profile whose record
changed behind the same reference is sent to the selected ready recipients again, with a fresh attempt count.

Add `profile.placeContactAvatars`. A chat App tells the host where it draws contacts' avatars (surface size and, per
avatar, a slot id, peer identity, square rect and clip), and the host draws the photo and mood ring of each contact who
shared a profile with it on its own layer. The core filters the placement to contacts with a current reference, hands
them with their references and `sharedAt` (Unix ms of the contact's share) to the new
`ProfilePlatform.placeContactAvatars(product, placed)` callback, and redraws the remembered placement when a reference
arrives, is re-shared or is withdrawn; a larger `sharedAt` for the same reference tells the host its cached profile is
stale; it clears it when the connection goes away. The product is answered `Ok` whoever shared; only a malformed
placement (more than 64 slots, a surface side outside 1 to 16384, a non-square avatar or one outside 1 to 1024 a side, a
repeated slot) is refused, and a host that cannot draw answers `Unsupported`. A JS host that supplies a `profile` group
must implement the callback; the Rust trait's default draws nothing.

Add `profile.ownStatus` and `profile.presentOwn`, and an optional `own` slot in version 2 of
`profile.placeContactAvatars`. A chat product can report whether its signed-in user has configured a profile and ask the
host to present it without receiving the bearer reference. The core fills the own slot from the user's disclosure and
hands it to the existing callback in the same replacement set as the contact avatars, redraws it when the user discloses
or retracts, and still reveals nothing per slot. Version 1 placements keep working unchanged. The avatar regression
suite also exercises version-1 response downgrading alongside the version-2 own-profile slot.

Version 2 of `profile.disclose` adds explicit `ChatApps`, `App { productId }`, and `Contacts { handles }` audiences.
App-scoped and selected-contact personal grants coexist: personal grants are host-renderable across products, never
returned to them. All handles are verified against the host Contacts lookup before committing the replacement; empty
audiences configure only the user's own profile. Existing V1 calls retain their app-scoped all-Chat behavior. Groups
remain product-owned sets of opaque handles, not a new host group API.

Personal relay uses distinct Chat content 22 (scope 1) and wallet/network-scoped `ProfilePersonalReferencesReceived`
storage. App content 21 is unchanged. Durable revisions, separate scoped watermarks and withdrawal tombstones prevent an
older personal share delivered through another app from reviving a withdrawn grant. Removing one audience does not
revoke an overlapping grant in another scope. Delivery still requires a ready authenticated Chat channel and a running
transport product; Contacts membership alone creates neither.

Version 2 of `profile.presentContact` accepts either a peer identity or a Contacts handle and hides profile
availability, including host rendering failures. V1 retains its app-only lookup and errors, so it cannot probe new
cross-app personal grants. Version 3 of `profile.placeContactAvatars` accepts the same selectors alongside the own slot;
V1/V2 placement bytes and replies remain compatible. Contacts-change notifications invalidate cached handle lookups and
refresh remembered avatars. App-specific references take precedence over personal ones; personal updates redraw all
affected wallet placements. Personal revisions also advance the host-rendered freshness timestamp when a newer share
arrives through an actor whose clock is older, preventing a same-reference update from leaving stale cached profile
contents.

Add `contacts.pickMany` with preselected opaque handles and explicit picked, dismissed, and no-contacts outcomes. Add
`contacts.placeLabels` so Apps can reserve host-rendered contact names without receiving those names or profile
availability. The core validates bounded placements and wallet-scoped handles, refreshes labels after Contacts changes,
and releases them when the connection closes. Hosts without a label surface return `Unsupported`; Worker products cannot
place labels. Clearing a session serializes removal of its remembered contact labels with pending refreshes. Host-side
interruption returns a Contacts domain error, reserving wire `Cancelled` for a peer's explicit cancellation. Failed
directory lookups preserve the prior label surface and report a retryable error instead of clearing it as if the
contacts were missing.
