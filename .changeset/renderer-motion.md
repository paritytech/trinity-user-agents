---
"@parity/truapi": minor
---

Renderer trees can carry the `Pan`, `Drive` and `Perspective` modifiers. A pan moves a value the host keeps on the device, a drive sets a node's rotation, scale, opacity or offset from that value every frame, and the pan reports `PanEvent`s (`Began`, `Settled`, `Cancelled`) as the SCALE-encoded payload of its action. Ask `featureSupported({ tag: "RendererMotion" })` before sending them: a host that does not draw motion fails to decode such a tree. No host answers yes yet.
