---
"@parity/truapi-host": minor
---

Name the contact who shared a presented profile. `profile.presentContact` now reaches the new
`ProfilePlatform.presentContactProfile(product, presented)` callback, where `presented` carries the `reference`, the
`peerIdentity` of the contact whose authenticated Chat device delivered it, and `sharedAt` (Unix ms of that share), so a
host can say who shared a profile rather than which product asked. It names who sent the reference, not whose profile it
is: the record is not signed by its owner, and a contact can forward someone else's reference. A JS host that supplies a
`profile` group must implement the callback; one built before it still has contacts' profiles presented through
`presentProfile`, as the Rust trait's default does. The product-facing Profile wire is unchanged.
