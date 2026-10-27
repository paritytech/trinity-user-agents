---
"@parity/truapi-host": patch
---

Native hosts screen a product-supplied chat room icon through the core rather than reimplementing it. `screen_chat_icon` applies the rules `create_room` and `register_bot` already apply, for a host to call wherever it takes a room icon from a product, and raises `NativeChatFieldError`.
