---
"@parity/truapi": major
---

The generated SDK drops historical payload types and SCALE codecs that none of its methods use. This removes exports such as `V01ChatRoomFooter`. If you import one of these for a product call, use the request or result type of the generated method. Methods that still use V1 and subscriptions that receive older host frames keep their codecs.
