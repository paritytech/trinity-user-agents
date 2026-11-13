---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Adds `RendererNode::SlideToConfirm`, a control the person slides to confirm, at variant index 12. The host owns the drag
and sends `confirm_action` with an empty payload once a slide completes, as a `Button` press does. A host built before
this release cannot draw a tree that uses it.
