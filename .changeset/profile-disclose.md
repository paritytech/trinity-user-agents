---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add `profile.disclose`, `profile.retract` and `profile.presentContact`. A product discloses one opaque reference to
the user's chat contacts and may withdraw it; a product names a contact by peer identity and the host presents the
reference that contact disclosed, so no product holds a contact's reference. Disclosed and received references live in
core storage (`ProfileDisclosure`, `ProfileReferencesReceived`); the chat relay that fills the latter is not part of
this change.
