---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A paired session resolves its dotNS username again for accounts onboarded more than a week ago. Pending gateway claims count however old they are, because the PoP controller no longer expires them. An account whose contract labels give no lite username, such as one onboarded before lite names were stored dotted, takes it from the gateway pallet's `DotnsGateway.AccountNames` record, accepted only when `DotnsGateway.LiteLabelOwner` names the same account.
