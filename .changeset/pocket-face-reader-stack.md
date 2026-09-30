---
"@parity/truapi-host": patch
---

`parse_renderer_node_json` reads a card face on a thread of its own with an 8 MiB stack, so a face nested as deep as the
reader admits is refused instead of overflowing the stack of the thread a host calls it from, which took the app down. A
face at that bound needs between 1 and 2 MiB, more than an Android background thread or an iOS secondary thread carries.
A host that cannot start that thread gets `NativeRendererError::ReaderUnavailable`. `decode_renderer_node` refuses bytes
past the end of the kept tree, as `Malformed`.
