---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

A host answers a feed update with a `ChatFeedOutcome`: `Applied`, or `UnknownMessage`, `DuplicateMessage` or `NotOwner`
for an update that it did not apply, which the product receives as the error of the same name. The web callback
`updateChatFeed`, `updateFeed` of the Swift and Kotlin bridges and `ChatPlatform::update_chat_feed` return it, and throw
only for a failure of the host, which the product receives as `HostFailure`.
