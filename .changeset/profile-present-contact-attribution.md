---
"@parity/truapi-host": minor
---

Name the contact in host-owned profile presentation, including a friendly empty state when no live reference has arrived.
`ProfilePlatform.presentContactProfile(product, presented)` receives `peerIdentity`, optional verified `username`,
and optional `shared: { reference, sharedAt }`. Absence of `shared` requests empty-profile feedback without a fetch.
The product-facing V2 reply does not reveal whether any information was available or displayed.
The username is the one the product's Chat roster verified for that contact, else the contact's verified dotNS name,
looked up for at most 2 seconds; it never comes from the product. It names who sent the reference, not whose profile it
is: the record is not signed by its owner, and a contact can forward someone else's reference. The default adapter
can present a shared reference through `presentProfile`; empty-profile feedback requires `presentContactProfile`.
Storage errors, invalid references and invalid handles are not misrepresented as absent sharing. The product-facing
Profile wire is unchanged.
