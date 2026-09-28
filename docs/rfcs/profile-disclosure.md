---
title: "Profile disclosure to chat contacts"
owner: "@corey-hathaway"
status: draft
---

# RFC — Profile disclosure to chat contacts

## Summary

A product hands the host one opaque profile reference for the user's chat contacts. The host relays it to each contact
over Chat v2 and keeps the references contacts relay back. A chat product then asks the host to show a contact's profile
by naming the contact, and the host presents the reference that contact disclosed through the existing `profile.present`
path. No product holds another user's reference.

## Motivation

`profile.present` shows a profile from a reference the calling product already holds. A chat product has no honest way
to hold one for a contact: the reference is a bearer capability, so a product that carries it can read, keep and forward
the profile, and can show any reference against any contact. The reference has to travel host to host and stay inside
the hosts, and Chat v2 leaves ordinary delivery to products.

## Requirements

- **Blind:** the disclosing product never learns who the contacts are.
- **Sealed:** no product reads a reference in transit or at rest, on either side.
- **Bound:** a presented profile is the one that contact's host sent, not one a product chose.
- **Stable:** a change to the referenced profile does not require relaying again.
- **Withdrawable:** the discloser can retract, and contacts drop what they hold.

## Approach

The design has five parts:

- The `Profile` trait gains `disclose`, `retract` and `present_contact`.
- `disclose` asks the user once per product before anything is stored.
- Core storage holds the user's disclosure and the references received per chat product.
- The Chat v2 actor relays disclosures through its host-private outbox.
- `present_contact` substitutes the stored reference into `present`.

### Trait

```rust
#[wire_trait(id = 22)]
#[crate::async_trait]
pub trait Profile: Send + Sync {
    /// Show the referenced profile in host-owned UI.
    #[wire(id = 0)]
    async fn present(
        &self,
        _cx: &CallContext,
        _request: HostProfilePresentRequest,
    ) -> Result<HostProfilePresentResponse, CallError<HostProfilePresentError>> {
        Err(CallError::unavailable())
    }

    /// Give the user's chat contacts this reference. App executions only.
    #[wire(id = 1)]
    async fn disclose(
        &self,
        _cx: &CallContext,
        _request: HostProfileDiscloseRequest,
    ) -> Result<HostProfileDiscloseResponse, CallError<HostProfileDiscloseError>> {
        Err(CallError::unavailable())
    }

    /// Withdraw the reference this product disclosed.
    #[wire(id = 2)]
    async fn retract(
        &self,
        _cx: &CallContext,
        _request: HostProfileRetractRequest,
    ) -> Result<HostProfileRetractResponse, CallError<HostProfileRetractError>> {
        Err(CallError::unavailable())
    }

    /// Show the profile a chat contact disclosed.
    #[wire(id = 3)]
    async fn present_contact(
        &self,
        _cx: &CallContext,
        _request: HostProfilePresentContactRequest,
    ) -> Result<HostProfilePresentContactResponse, CallError<HostProfilePresentContactError>> {
        Err(CallError::unavailable())
    }
}

pub struct HostProfileDiscloseRequest {
    /// Opaque reference, screened like a `present` reference.
    pub reference: String,
}
pub enum HostProfileDiscloseError {
    /// The reference is empty, too long, or not printable ASCII.
    InvalidReference,
    /// The user declined, now or earlier, to let this product disclose a profile.
    PermissionDenied,
    /// No user is signed in.
    NotConnected,
    /// Catch-all.
    Unknown { reason: String },
}
pub enum HostProfileRetractError {
    /// Another product disclosed the reference the host holds.
    NotDiscloser,
    /// No user is signed in.
    NotConnected,
    /// Catch-all.
    Unknown { reason: String },
}
pub struct HostProfilePresentContactRequest {
    /// The contact's authenticated root identity, as the Chat v2 API names it.
    pub peer_identity: [u8; 32],
}
pub enum HostProfilePresentContactError {
    /// The contact has not disclosed a profile to the user.
    NotShared,
    /// The stored reference no longer passes screening.
    InvalidReference,
    /// No user is signed in.
    NotConnected,
    /// Catch-all.
    Unknown { reason: String },
}
```

### Consent

Every contact receives the reference, so a product may disclose only once the user has allowed it. The first `disclose`
from a product raises `UserConfirmationReview::ProfileDisclosure { product_id }` through the host's
`confirm_permission`, beside `ChatAuthority`; the answer is remembered per product as
`PermissionAuthorizationRequest::ProfileDisclosure`, and a refusal, then or remembered, is `PermissionDenied` with
nothing stored. `retract` never asks: withdrawing only narrows what contacts hold.

### Storage

Two core-storage slots hold references, and neither is visible to products. Both are scoped to the signed-in wallet and
the Chat network, as the Chat roster is. `ProfileDisclosure { root_public_key, genesis_hash }` holds the disclosing
product id and the reference. `ProfileReferencesReceived { root_public_key, genesis_hash, product_id }` holds, per chat
product, what each contact's host last sent: its discloser, its frame timestamp, and the reference, or `None` once
withdrawn. Clearing the product clears it. Hosts treat both as secret material.

### Relay

A disclosure travels as a new Chat v2 content type, `ProfileReference { discloser_product_id, reference: Option }`,
where `None` withdraws. The Chat actor seals it to each ready peer's devices through the same host-private outbox that
carries payments and rich files, so the chat product submits and retries opaque ciphertext it cannot read, and cannot
prepare the content type itself. A per-peer watermark records what was last sent; each reconcile sends the current
disclosure to every peer whose watermark differs, which covers the first share, a new contact, a replacement and a
withdrawal. On receipt the host screens the frame, stores it for that peer and removes it from the plaintext returned to
the product. Frames from compacted history are dropped.

The product decides the order it opens statements in, so frames are ordered by their timestamp, not by arrival. Each
frame a host sends a peer is timestamped later than the one before it, even if its clock steps back. The receiving host
applies a frame only if it is strictly newer than the one it holds, and keeps a withdrawal as a row rather than deleting
it, so a disclosure opened after its own withdrawal cannot bring the reference back.

Delivery is best effort. References have their own outbox budget, one per peer, so they never take a slot payments or
rich files need, and a reference that finds no room waits for a later reconcile rather than failing the chat product's
initialization. A queued reference is offered for one statement lifetime and then dropped without being re-signed: a
host that predates the content type rejects the whole statement and never acknowledges it.

Stability comes from the reference format rather than the relay: a reference that names a mutable record, such as a
registry slot, keeps working when the record changes, so a relay happens only when the reference itself changes.

### Presentation

`present_contact` looks up the caller's received reference for the named peer, screens it again, and hands it to
`ProfilePlatform::present_profile`. Host adapters are unchanged: they see a `present` whichever method produced it.

## Trade-offs

- One reference for all contacts, so withdrawing it from one contact means rotating it for all of them.
- A retraction cannot make a contact's host forget a reference it already resolved.
- The watermark advances when the message is queued, so a message that never arrives, or that a peer's host does not
  acknowledge within one statement lifetime, is not resent until the disclosure changes.
- Dropped: carrying the reference in ordinary chat content, which puts a bearer capability in product hands.

## Open questions

- The content-type index. The prototype uses V2 index 21, which native Chat has to agree to.
- Several disclosing products. There is one `ProfileDisclosure` slot, so the last product to disclose replaces the
  others and the earlier one can no longer retract. The alternative is one slot per product, with the host relaying the
  one from a product the user designates, as RFC 0024 designates a personhood provider.
- Consent covers the product, not the reference: once allowed, a product may replace its disclosure without asking.
- Devices. Only the host that took `disclose` knows the disclosure, so contacts that reach the user's other devices are
  not sent it.
- Reconcile timing. The relay runs when the chat product initializes, not when `disclose` returns.
- Resolution. Hosts parse references today; a shared resolver in the core would need the reference format specified here
  rather than by the publishing product.
