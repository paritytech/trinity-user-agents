---
"@parity/truapi": major
"@parity/truapi-host": major
---

`RendererNode::Materials` replaces `Effect`, which is removed with `EffectProps` and `Rainbow`. A `Materials` node names
materials by ID with their arguments and holds the content they draw over. Hosts draw it as its children for now, and
refuse a tree that still carries `Effect`.

`system.featureSupported` takes a `Materials` question (format version, features). Products ask it before
sending materials. A host answers yes only when it draws them. A host built before this release cannot read the
question and answers with an error, which products read as no.

`renderer.failureSubscribe` reports `Materials` nodes that fell back, on a host that answers yes.

Native hosts map `RendererNode.materials` and answer `HostFeatureSupportedRequest.materials` in their exhaustive
switches.
