---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add `chat.onOpenLink`, chat method 28: the host gives a product what the user opened from an entry point of the host that only the product reads, such as a contact link, a scanned code or a username, together with a room of the product. The product answers the room the host shows next. A product without a handler answers `Unsupported`, and the host shows the room of the request. Native hosts call `NativeProductExecution.chat_open_link`, web hosts `chatOpenLink`.
