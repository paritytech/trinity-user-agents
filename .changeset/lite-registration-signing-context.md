---
"@parity/truapi": patch
---

The CLI host signs lite username registrations in the form the network's runtime accepts. On individuality runtimes above spec version 3004000, the candidate and consumer registration signatures and the dotNS reservation signature cover the personhood signing context followed by the message, as paritytech/individuality-community#196 requires. Earlier runtimes keep receiving signatures over the message alone, so the same release works before and after the runtime upgrade.
