---
"@parity/truapi": minor
---

The `truapi-host dev` chat page serves Chat Extension v2. The in-memory chat host keeps what a product declares about
each room (metadata with its icon and empty-room text, configuration, preview, footer) and stores the messages of rooms
it keeps, applying `update_feed`, while a room the product keeps is paged from the product with `messages_query` and
takes the user's sends, edits, deletions, reactions and seen reports through `user_action`, within the time-outs of the
RFC. A message the page sends to such a room carries a message id and a time the page chose. The page draws the chat
list with pinned rooms first, previews, activity lines, badges (a count or an image) and unseen reactions, the
empty-room text, every message body kind, status, reactions, replies, the message menu, who reacted and the footer,
draws a `Replaced` message again in place, and removes a deleted one. A room the host keeps takes the user's messages
only; edits and reactions there come from the product.
