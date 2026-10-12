---
"@parity/truapi": major
"@parity/truapi-host": major
---

`preimage.submit` raises no per-upload confirmation. The `PreimageSubmit` permission authorizes it: `AllowAlways` covers every upload, and `AllowOnce` covers a single upload. Allocating or renewing the Bulletin allowance may still ask the paired wallet. `UserConfirmationReview` has no `PreimageSubmit` variant and `PreimageSubmitReview` is not exported, so hosts that match on either remove that case; the remaining review variants keep their wire discriminants.
