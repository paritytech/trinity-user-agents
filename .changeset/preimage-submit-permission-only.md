---
"@parity/truapi": major
"@parity/truapi-host": major
---

Keep upload consent separate from the `PreimageSubmit` capability grant. An upload requires host-owned review unless an explicit bounded automatic-upload policy approves it. Retain `UserConfirmationReview::PreimageSubmit` and `PreimageSubmitReview` for host integrations. Allocating or renewing the Bulletin allowance may also require approval.
