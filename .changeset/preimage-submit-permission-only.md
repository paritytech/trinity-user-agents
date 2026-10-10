---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

`preimage.submit` no longer asks the user to confirm each upload. The `PreimageSubmit` permission authorizes it: an always grant covers every upload, and a one-use grant covers one upload. Bulletin allowance checks and renewal are unchanged. Hosts no longer receive `UserConfirmationReview::PreimageSubmit`.
