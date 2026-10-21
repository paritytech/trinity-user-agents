---
"@parity/truapi": minor
---

The `truapi-host dev` chat page serves Chat Extension v2. The in-memory chat host keeps what a product declares about
each room (metadata, configuration, preview, footer) and stores the
messages of rooms it keeps, applying `update_feed`, while a room the product keeps is paged from the product with
`messages_query` and takes the user's sends, edits, reactions and seen reports through `user_action`, within the
time-outs of the RFC. The page draws the chat list with previews and badges, every message body kind, status, reactions,
replies, the message menu from the product's query, who reacted, the participant list and every footer variant.
