---
"@parity/truapi-host": minor
---

The test host's chat user sends attachments with `sendAttachment`: inline up to 1 MiB, else staged through
`stageChatAttachment`, answering `Refused` and sending nothing when the core refuses.
