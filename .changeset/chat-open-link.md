---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add `chat.onOpenLink`, chat method 28: the host gives a product the payload of a room link the user opened, the text after `#` in `polkadot://<product>/-/chat/<room>#<payload>` or in its web form `https://<name>.<root>.li/-/chat/<room>#<payload>`. The product answers the room the host shows next. A host lists the feature as `room_link_payloads`. A product without a handler answers `Unsupported`, and the host shows the room of the link without the payload. Native hosts call `NativeProductExecution.chat_open_link`, web hosts `chatOpenLink`.
