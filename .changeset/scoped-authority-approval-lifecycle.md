---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Product clearing revokes only that product's in-flight signing, resource allocation, identity disclosure and contact authority, while account replacement or disconnection revokes all prior authority. Resource reviews and allowance-cache commits revalidate their product and account. Identity disclosure retains identity-owner checks and lookup-error propagation, and contact selection and labels retain independent directory-mutation fences. Cancelled CLI signing requests withdraw their permission and signature reviews, including on product disconnection, without authorizing late answers or losing the command draft.
