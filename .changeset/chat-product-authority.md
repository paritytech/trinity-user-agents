---
"@parity/truapi-host": minor
---

Add product-scoped Chat v2 authority with dedicated Chat authorization, separate from username disclosure. Apply the
boundary to local and SSO sessions, expose it in the iOS permission flow, and avoid cloning secret-bearing pairing
results.

Support keyless Statement Store allowances for a selected product account in the native signing host.

Replace raw guest Chat crypto with a Host-owned native actor on account method 12;
retire method 11, including over SSO. Keep main-purse Coinage secrets and durable
claim/recovery plans behind trusted per-payment review. Add private nested HOP
history recovery and resumable native file/image/video attachments with trusted
selection/export, metadata-only guest views and live Bulletin endpoint/session
fences. The combined signing runtime includes AGPL-3.0-only code; retain the
included provenance, licenses and exact Corresponding Source.

Include the complete native Chat wire, attachment and cryptography implementation
as the source-owned `truapi-chat-v2` crate, with upstream provenance and licensing.
Remove the release dependency on unpublished local Cargo overrides.

Align Coinage keys with current iOS MAIN_PURSE/page-0 derivations, including the
soft coin item junction. Keep complete exported coin secrets stable for durable
payment replay. Authenticate the new purse layout through snapshot version 3;
reject legacy `//pps` snapshots without discarding pending wallet state. Native
iOS allocator sharing remains a separate integration requirement.

Read origin-specific free Coinage unload-token limits from the runtime view at
the finalized planning snapshot, rather than a removed metadata constant.
Recover full Statement Store accounts by refreshing only the signed statement's
priority while preserving committed ciphertext and payment IDs; continue other
peers' queued deliveries when one account or channel is blocked.

Accept validated native push-token metadata without discarding the surrounding
iOS acceptance batch. Keep token credentials out of guest history and storage,
while binding the complete metadata frame into authenticated replay detection.

Match native iOS acknowledgment direction on identity and device sessions.
Subscribe to peer-originating routes and encrypt replies on the Host's own
outgoing route. Repair previously queued reverse-route acknowledgments on
authenticated replay while preserving payment and message commitments.

Allow outgoing payments once an active, keyed peer device acknowledges the
legacy-device revocation update. Encrypt only for acknowledged devices and
reject payment acknowledgments from devices excluded from the committed envelope.
Reset eligibility after authenticated roster changes while preserving payment
identity and exact memo custody through rewrapping and retries.

Document the method 12 request/response, compatibility, custody, device
eligibility, and per-spend consent contract in the unnumbered
[draft native Chat/main-purse RFC](../docs/rfcs/native-chat-main-purse.md),
submitted for review with this implementation. Clarify its relationship to
[RFC 0017](../docs/rfcs/0017-coinage-payment.md), including the distinct integer
amount/asset contracts and the absence of general purse APIs or a Chat balance
query. Treat same-wallet competing native/Rust allocators as a release blocker.
This specification link does not assert RFC approval, publication, or
cross-platform qualification.
