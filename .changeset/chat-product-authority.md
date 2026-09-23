---
"@parity/truapi-host": minor
---

Add product-scoped Chat v2 authority with dedicated Chat authorization, separate from username disclosure. Apply the
boundary to local and SSO sessions, expose it in the iOS permission flow, and avoid cloning secret-bearing pairing
results.

Support keyless Statement Store allowances for a selected product account in the native signing host.

Replace raw guest Chat crypto with a narrow authenticated Host boundary on account method 12; retire method 11,
including over SSO. Products own ordinary Chat, subscriptions, transport and ACKs. Keep identity/device keys and
outgoing payment secrets private; require trusted per-payment review for main-purse debits. Incoming bearer keys may
enter encrypted product recovery storage and are imported through generic `payment.top_up(Coins)`. Retain private nested
HOP recovery and resumable file/image/video preparation with trusted selection/export. The combined signing runtime
includes AGPL-3.0-only code; retain the included provenance, licenses and exact Corresponding Source.

Include the complete native Chat wire, attachment and cryptography implementation as the source-owned `truapi-chat-v2`
crate, with upstream provenance and licensing. Remove the release dependency on unpublished local Cargo overrides.

Align Coinage keys with current iOS MAIN_PURSE/page-0 derivations, including the soft coin item junction. Keep complete
exported coin secrets stable for durable payment replay. Authenticate the new purse layout through snapshot version 3;
reject legacy `//pps` snapshots without discarding pending wallet state. Inject native wallet custody at runtime
construction: reference iOS supplies its existing Coinage service adapter; browser/CLI use Rust when none is registered.

Read origin-specific free Coinage unload-token limits from the runtime view at the finalized planning snapshot, rather
than a removed metadata constant. Preserve committed payment ciphertext and native request IDs when renewing statement
expiry. Statement submission, backpressure and ordinary retry queues belong to the product.

Accept validated native push-token metadata without discarding the surrounding iOS acceptance batch. Authenticate native
identity/device ACK direction and reject own-signer ciphertext reflections before opening private payloads.

Use request/response V2 and encrypted SSO V3; reject the former actor request rather than returning fake compatibility
responses. Provide bounded replayable HOP and public-state pages with original native IDs. Transfer legacy ordinary
history, pending ciphertext and caller/native ID mappings only after explicit durable migration acknowledgment.

Import incoming keys through the selected owner's durable custody: the encrypted Rust main-purse WAL or native
IncomingPaymentService. Recover owner-bound accepted imports after unlock independently of Chat grants or a running
guest. Expose trusted denomination metadata without opening a wallet allocator: native memo totals and top-up minimums
are raw u128 chain units, while Chat cards use checked native cents. Preserve exact import amounts on retries and do not
report ambiguous or underfunded claims as fully cleared.

Allow outgoing payments once an active, keyed peer device acknowledges the legacy-device revocation update. Encrypt only
for acknowledged devices and reject payment acknowledgments from devices excluded from the committed envelope. Reset
eligibility after authenticated roster changes while preserving payment identity and exact memo custody through
rewrapping and retries.

Document the method 12 request/response, compatibility, custody, device eligibility, and per-spend consent contract in
the unnumbered [draft native Chat/main-purse RFC](../docs/rfcs/native-chat-main-purse.md), submitted for review with
this implementation. Clarify its relationship to [RFC 0017](../docs/rfcs/0017-coinage-payment.md), including the
distinct integer amount/asset contracts and the absence of general purse APIs or a Chat balance query. Keep the native
Rust-purse storage guard: the adapter does not permit competing allocators or automatic fallback. This specification
link does not assert RFC approval, publication, native iOS compilation, or cross-platform qualification.

Mark native Chat payments as transitional pending RFC 0017 receivables, encrypted cheques, and deposits using the main
purse. Preserve current native-peer payments and pending custody without claiming RFC 0017 support or introducing a
separate Chat allocator. The Host no longer runs ordinary Chat receivers when the product is closed.

Generate current request/success envelopes with compatible older domain-error envelopes instead of silently omitting the
method. Preserve wire discriminants after retiring a version prefix, and exercise the generated client against V2 Chat
responses and V1 errors.

Add an optional Host-private native-wallet dependency, fixed at runtime construction. Absence uses Rust by default;
registered native failures never switch owners. Remove the explicit wallet-selection callback and enum. Route reference
iOS outgoing payments, incoming imports and status through the coordinator's existing Coinage service. Retain native
outgoing custody atomically with native transaction registration before exposing a memo to trusted Host crypto, then
track ciphertext acceptance, peer delivery and finality separately. Preserve per-payment and privacy review,
source/idempotency bindings, activation fencing and authoritative native wallet balance. Native callback infrastructure
errors are sanitized; product callback overrides cannot replace the runtime-wide wallet owner.

Bind native incoming receipts to wallet root, chain and asset instance in the additive Core Data schema upgrade.
Preserve unowned pre-upgrade incoming rows and source secrets without automatically claiming them into the current
wallet. Pending imports without provable ownership require explicit ownership recovery before resumption.
