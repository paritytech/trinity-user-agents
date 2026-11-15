---
"@parity/truapi": major
"@parity/truapi-host": major
---

A product can show a page of its own web content in the footer of a room it created by setting the footer to `Web` with a `ChatRoomWebFooter` (`path`, `height`). The path is relative to the product's own web root and may carry a query string and a fragment, but no scheme, host or parent reference. The height is in logical points, and the host caps it.

The host loads the page from the product's own origin, the same way it loads the product's pages, in place of the text field. A press the page reports reaches the product's `rendererActionSubscribe` with the `ChatRoomFooter` render context and an empty `message_type`.

Native hosts handle the `Web` case of `ChatRoomFooter`. An older host cannot decode `Web` and fails the call, so a product can fall back to a `Card` there.
