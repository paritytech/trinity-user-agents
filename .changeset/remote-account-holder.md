---
"@parity/truapi-host": patch
---

A paired host sends each wallet operation through `RemoteAccountHolder`, which turns it into an SSO request and keeps the paired copy of the phone's ring-VRF registrations. Paired session changes hold back the host's grant writes: a login, a restored or external session and a sign-out finish revoking the previous session's grants and clearing its stored auth session before the next session is installed, and a deletion that failed is retried first. A login that another activation replaced is not installed, and activating an external session reports an error when the previous session's cleanup fails. A cancelled SSO request is withdrawn from the phone even when the wait for its answer is dropped.
