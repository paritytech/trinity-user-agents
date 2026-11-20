---
"@parity/truapi": minor
---

The core resolves `Materials` nodes for a chat message card, `RenderContext::ChatMessage`, beside the two Pocket card contexts, when the host turns materials on. The room footer still draws them plain and reports `Unsupported`. A host that draws chat cards with materials off sees no change.
