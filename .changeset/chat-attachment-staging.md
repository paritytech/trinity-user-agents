---
"@parity/truapi-host": minor
---

The core stages chat attachments. `stageChatAttachment` stores a user attachment above the inline bound as one Bulletin
preimage that the product's own allowance pays for, under the product's `PreimageSubmit` permission, and answers its
key. It refuses a room whose features do not declare `preimage_attachments` (read from the last declaration the core
accepted, else through the product's chat adapter), an attachment above 8 MiB or above the Bulletin chain's
per-transaction bound read from its runtime, a refused permission and a missing allowance; nothing is uploaded then.
`capabilities` lists `preimage_attachments` with its 8 MiB bound wherever the host lists it.
