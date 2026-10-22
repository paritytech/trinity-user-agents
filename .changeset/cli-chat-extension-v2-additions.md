---
"@parity/truapi": minor
---

The `truapi-host dev` chat page serves the Chat Extension v2 additions. The in-memory chat host lists the features it
serves (`product_source_rooms`, `reaction_details`, `call_buttons`, `contact_rooms`) and shows them as a Capabilities
line on the page. It keeps each room's declared features whole (the room header marks a feature it does
not serve), the call of each room and the contact of each room. A room with a contact is drawn as a person-to-person
chat: requests are listed apart, the footer follows the contact state (invite composer, waiting line, Accept and
Decline, Unblock, left the chat), and the header menu blocks, unblocks and leaves; each action reaches the product as a
`user_action`. The page draws `Event`, `Call` and `Payment` bodies and the request message, redraws a `Replaced`
message in place, shows the edit history of a message, and opens a link through `open_link`. A message the page sends
carries a message id and a time the page chose, and the list shows a removed reaction and marks unseen reactions. On `/chat/ws` the
`contact`, `editHistory` and `openLink` operations do the same for a script, `post` takes an optional `messageId` and
`timestamp`, `callRequest` an optional `callId`, and `scripts/lib/cli-chat-socket.mjs` drives them.
