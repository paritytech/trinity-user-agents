---
"@parity/truapi": minor
---

The `truapi-host dev` chat page serves Chat Extension v2. The in-memory chat host keeps what a product declares about
each room (metadata, configuration, preview, footer, toolbar, quick replies, commands, activity line) and stores the
messages of rooms it keeps, applying `update_feed`, while a room the product keeps is paged from the product with
`messages_query` and takes the user's sends, edits, reactions and seen reports through `user_action`, within the
time-outs of the RFC. The page draws the chat list with previews and badges, every message body kind, status, reactions,
replies, the message and participant menus from the product's queries, who reacted, the participant list, the toolbar,
every footer variant, the quick replies, the `/` command catalogue and the activity line, and sends `RoomOpened` and
`RoomClosed` as rooms open and close. `make e2e-chat-cli` runs the `playground/chat-v2` sample product on the page in
headless Chromium and writes a diagnosis row per scenario it covers.
