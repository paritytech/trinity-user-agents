---
title: "Profile disclosure to chat contacts"
owner: "@corey-hathaway"
status: draft
---

# RFC — Profile disclosure to chat contacts

## Summary

A product hands the host one opaque profile reference and an audience policy. The host relays app-scoped grants or
personal grants to selected Contacts handles over authenticated Chat v2 channels, and keeps received references
inside the host. An app-scoped grant is renderable in that app; a personal grant is renderable across apps on the
recipient's host. Products request host-owned drawers and avatar overlays by peer identity or opaque handle, never
receive another user's reference, and cannot inspect the host's rendering.

## Motivation

`profile.present` shows a profile from a reference the calling product already holds. A chat product has no honest way
to hold one for a contact: the reference is a bearer capability, so a product that carries it can read, keep and forward
the profile, and can show any reference against any contact. The reference has to travel host to host and stay inside
the hosts, and Chat v2 leaves ordinary delivery to products.

## Requirements

- **Blind:** products may retain selected opaque handles, but receive no contact names, accounts or contact enumeration.
- **Sealed:** no product reads a reference in transit or at rest, on either side.
- **Bound:** a presented profile is the one that contact's host sent, not one a product chose.
- **Stable:** a change to the referenced profile does not require relaying again.
- **Withdrawable:** narrowing an audience withdraws its grant; recipients stop rendering it once the withdrawal arrives.
- **Unobservable:** a product that shows contacts' avatars cannot tell which contacts shared a profile.

## Approach

The design has six parts:

- The `Profile` trait gains `disclose`, `retract`, `present_contact` and `place_contact_avatars`.
- `disclose` asks the user once per product before anything is stored.
- Core storage holds the user's disclosure, app-scoped received references and wallet-wide personal received references.
- The Chat v2 actor relays disclosures through its host-private outbox.
- `present_contact` hands the host the stored reference and the contact who sent it.
- `place_contact_avatars` substitutes stored references into a host-drawn avatar layer.

### Trait

`Profile` uses wire trait **69**. This change preserves that address and the existing method IDs.

| Method | ID | Request versions |
| --- | --- | --- |
| `present` | 0 | V1: reference supplied by the caller |
| `disclose` | 1 | V1: reference; V2: reference plus audiences |
| `retract` | 2 | V1 |
| `present_contact` | 3 | V1: peer identity; V2: peer or Contacts handle |
| `place_contact_avatars` | 4 | V1: peer slots; V2: optional own slot; V3: peer/handle slots plus own |
| `own_status` | 5 | V1 |
| `present_own` | 6 | V1 |

The canonical payloads are in `truapi::latest`; wire envelopes are in `truapi::versioned::profile`.
The new audience and selector shapes are:

```rust
pub enum ProfileAudience {
    ChatApps,
    App { product_id: String },
    Contacts { handles: Vec<ContactHandle> },
}
pub struct HostProfileDiscloseRequest {
    pub reference: String,
    pub audiences: Vec<ProfileAudience>,
}
pub enum ProfileContact {
    Peer { peer_identity: [u8; 32] },
    Handle { handle: ContactHandle },
}
pub struct HostProfilePresentContactRequest {
    pub contact: ProfileContact,
}
pub struct ContactAvatarSlot {
    pub slot: u32,
    pub contact: ProfileContact,
    pub rect: AvatarRect,
    pub clip: AvatarRect,
}
```

V1 disclosure maps to `ChatApps`: app-scoped sharing with ready peers of each Chat app, not a wallet-wide personal
grant. `App` selects one normalized product ID. `Contacts` selects personal recipients by opaque handle. A request may
combine these; an empty list retains the own profile without granting delivery. Calls replace the previous audience
policy. At most 64 audience entries and 4096 handles are accepted; duplicates are coalesced.

The core resolves handles using the same verified Contacts lookup as transaction recipient substitution, rehashes
returned accounts, and rejects the entire disclosure if any handle is invalid or the session/cache generation changes.
It never guesses a translation between a payment account, device key and Chat root identity: a resolved account must
exactly match an authenticated ready peer identity. A Contacts entry does not establish such a channel.

Seity groups can be sets of handles whose union is passed as `Contacts`. Group names, labels and membership editing
are not host Profile state. Handles are stable pseudonyms across apps and hosts for one user, not unlinkable identities.

### Consent

The first `disclose` from a product raises `UserConfirmationReview::ProfileDisclosure { product_id }` through the host's
`confirm_permission`, beside `ChatAuthority`; the answer is remembered per product as
`PermissionAuthorizationRequest::ProfileDisclosure`. A refusal is `PermissionDenied` with nothing stored.
The current review authorizes the disclosing product, not each audience mutation. A product must explain the difference
between sharing inside an app and personal sharing across apps; audience-specific host consent remains a rollout
question. `retract` never asks: it withdraws all grants of the current disclosure.

### Storage

Three secret core-storage slots are scoped to the signed-in wallet and People network:

- `ProfileDisclosure { root_public_key, genesis_hash }`: discloser, reference, audience policy and durable revision.
  Retraction retains a revision tombstone, so the next share cannot reuse an older sequence after restart.
- `ProfileReferencesReceived { root_public_key, genesis_hash, product_id }`: app-scoped received grants and withdrawals.
- `ProfilePersonalReferencesReceived { root_public_key, genesis_hash }`: personal received grants and withdrawals,
  shared across recipient apps but isolated from other wallets and networks.

Legacy disclosure and watermark records migrate to app-scoped behavior, never to personal grants. A live app-specific
reference takes precedence over a personal one. An app withdrawal removes only that grant, allowing a personal grant
to remain visible; a personal withdrawal leaves app grants intact.

### Relay

App grants retain Chat v2 content **21**, `ProfileReference { discloser_product_id, reference: Option }`, byte for byte.
Personal grants use distinct content **22**, with validated personal scope byte **1**, a nonzero durable disclosure
revision, the disclosing product and optional reference. `None` withdraws in that scope only.
The Chat actor seals frames to ready peer devices through the host-private outbox. The product submits opaque
ciphertext, cannot prepare profile content itself, and receives no profile content in opened history.
Per-peer, per-scope watermarks track shares, replacements and withdrawals. Personal frames are addressed only to selected
resolved accounts. Frames from compacted history are dropped.

A Chat actor publishes:

- when the chat product initializes;
- at the start of each reconcile, which heals any trigger that was missed;
- after any Chat request in which a peer became ready, such as the acknowledgement that completes a device handover, so
  that request's response already carries the reference;
- when `disclose` or `retract` changes the disclosure while the chat is open. The core stores the change, answers the
  call, and asks every open Chat actor of the same wallet and network, whatever its product, to publish on a task of its
  own, so the call never waits on it.

A wallet/network profile-state gate serializes disclosure replacement, publication and received-store updates.
Without a new disclosure and with nothing lapsed, publication reads the disclosure and checks watermarks.
Narrowing an audience removes obsolete unsent frames, even for peers that are no longer ready, and retains withdrawal
watermarks for later delivery. Already returned signed frames cannot be recalled.

App frames retain timestamp ordering. Personal frames use the durable disclosure revision across actors: independent
app clocks must not allow an old share to undo a newer withdrawal. Received withdrawals remain tombstones, so replaying
an older share cannot restore it after the newer withdrawal has been received.

Delivery is best effort. References share the existing bounded profile outbox budget, with separate entries per peer
and scope, and never take slots reserved for payments or rich files. A frame that finds no room waits for a later
publish. An unacknowledged frame lapsing after one statement lifetime is signed again for a ready peer, up to three
frames per scope and disclosure; a new disclosure starts a fresh count. Hosts predating a content type reject it and
never acknowledge it. Migration retains existing app watermarks and pending withdrawals.

The host only prepares statements: the chat product submits them. A publish outside the product's own requests, after
`disclose` or `retract`, queues the reference while the chat actor is open, and it reaches the contact once the chat
product next runs and submits what its responses offer.

A reference may name a mutable record. Every `disclose`, even with an unchanged reference, advances the durable
revision and starts a new round for the selected recipients with fresh attempt counts. Initialize, reconcile and
readiness-triggered publishes do not resend a round already delivered. A pre-revision disclosure reads as revision 0
and preserves its legacy app digest, so migration alone does not broaden or resend it.

### Presentation

V1 `present_contact` reads only the caller's app-scoped grant and preserves its existing errors. It cannot probe personal
grants through `NotShared`. V2 accepts a peer or verified Contacts handle, selects the live app grant then personal
fallback, and answers uniformly for an absent, unknown or unreadable profile, including host drawing failures.
Neither path returns the reference. The core calls
`ProfilePlatform::present_contact_profile(product, PresentedContactProfile { shared, peer_identity, username })`.
`shared` is `Some(SharedContactProfile { reference, shared_at })` for a live reference and `None` for a genuinely absent
or retracted V2 reference. The host opens friendly empty-profile feedback for `None`, without claiming that unreadable
storage or an invalid reference means nothing was shared. `shared_at` is a frame freshness timestamp; personal grants
advance it monotonically even when a newer revision arrives from an actor with an older clock. `username` is the host's
own name for the contact, never one from
the product: the name the calling product's Chat roster holds for
that peer, verified when the contact was bound or first authenticated, else the peer's verified dotNS name. The core
waits at most 2 seconds for it and passes `None` when it knows none, so a slow directory never holds the drawer back; the
host then names the contact generically, never by address. The default can call `present_profile` for a shared reference;
empty-profile feedback requires the contact presenter. The core rechecks the wallet and handle generation before either
presentation, so a late lookup cannot open another wallet's contact.

### Provenance

The host stores a received reference only when it arrives over the authenticated Chat v2 channel from that peer's own
device, so when `present_contact` opens the drawer the host knows who sent it: it can say "shared with you by <contact>
over Chat" and name that contact, not the product that asked. That is all it guarantees. The contacts record behind the
reference is not signed by its owner, so the reference proves who delivered it, not whose profile it is: a contact can
forward another person's reference as their own. Nor can copies be erased: a reference is a bearer capability, so
whoever received it, directly or forwarded, keeps it and what it resolved; a retraction only stops a receiving host from
presenting it.

`own_status` reports only whether the signed-in wallet has a current disclosure. `present_own` resolves that disclosure
and hands it to the same host presenter. Neither method returns the reference or profile contents to the product.

### Placed avatars

A chat product draws its own conversation list and header, so only it knows where each contact's avatar sits. It sends
`place_contact_avatars` with its surface size and, per avatar, a slot id, the contact's peer identity, the circle's
square bounding box and the region it is cut to, in surface units. Each call replaces the product's placement.

The core keeps only slots with an effective live app or personal reference, withdrawals excluded, and hands them to
`ProfilePlatform::place_contact_avatars(product, PlacedAvatars { surface_width, surface_height, avatars })`.
Each avatar carries `shared_at`, a monotonically advancing freshness timestamp within its grant scope. A newer value
for the same reference means the host should drop cached profile contents. The host draws each photo and mood ring, when
they have one, on a layer over the product that lets pointer input through; a tap still reaches the product, which
opens the profile with `present_contact`. The default callback draws nothing, so a host draws avatars only once it
implements it.

The core remembers the last placement per product connection, in memory. When a reference for that product arrives, is
re-shared in a newer frame or is withdrawn it filters the same geometry again and calls the host again, so avatars appear and disappear without the
product sending anything. Disposing the connection, or a placement made after the user signed out, clears what the host
drew.

Version 2 of `place_contact_avatars` adds an optional `own` slot for where the product draws the signed-in user's own
avatar. The core fills it from the wallet's current disclosure, with `shared_at` set to the disclosure's revision, and
hands it to the host in the same `PlacedAvatars` set as the contact avatars, so one placement never replaces another's
overlay. Slot ids are unique across `own` and the contact slots. Disclosing or retracting redraws every remembered
placement for that wallet, as a contact's reference change does. A version 1 placement is one with no own slot.

Version 3 retains the own slot and accepts `ProfileContact` selectors for contact slots. V1/V2 requests and replies
remain compatible. Handle placements revalidate the session and Contacts cache generation on redraw.
`notifyContactsChanged` clears stale handle resolution and clears the old overlay before resolving it again, so
removed handles cannot leave old avatars visible. Personal receives and withdrawals refresh all wallet placements.
Removing a Contacts entry invalidates lookup but does not itself edit an already approved disclosure's recipient set.

No leak: the product must not learn who shared a profile. The core answers `Ok` to any well-formed placement from a
signed-in user however many avatars, if any, are drawn; it returns nothing per slot, logs nothing about slots, and
treats a host drawing failure as success, since it could depend on which avatars were drawn. Only what the product
itself controls is refused: more than 64 slots, a surface side outside 1 to 16384, an avatar that is not square or is
outside 1 to 1024 a side, or a repeated slot id. The one host answer passed on is `Unsupported`, a property of the host
rather than of any contact. Nothing drawn is posted back to the product; the host renders it where the product cannot
read it.

## Trade-offs

- One reference is shared by all audiences. A narrower audience withdraws rendering only for the removed grants;
  overlapping grants remain effective. It cannot invalidate copies of the bearer reference.
- A retraction cannot make a contact's host forget a reference it already resolved.
- The watermark advances when the message is queued. A message that never arrives is sent again only when it lapses
  unacknowledged, three frames at most per disclosure, so a contact whose host misses all three is not sent it again
  until the disclosure changes.
- The chat product must run to submit what the host prepares. A disclosure changed while no chat product runs is relayed
  when one next initializes.
- The host layer covers the product's own drawing, so a product that animates or scrolls between placements shows the
  avatar a frame late; the product re-sends its placement when the list moves.
- Dropped: carrying the reference in ordinary chat content, which puts a bearer capability in product hands.

## Open questions

- Chat content indices 21 (app) and 22 (personal) require coordination with native Chat before rollout.
- Several disclosing products. There is one `ProfileDisclosure` slot, so the last product to disclose replaces the
  others and the earlier one can no longer retract. The alternative is one slot per product, with the host relaying the
  one from a product the user designates, as RFC 0024 designates a personhood provider.
- Consent covers the product, not each audience mutation: once allowed, a product may replace its disclosure without
  asking. Selected-contact and cross-app personal-sharing review must be agreed with the host Contacts owner.
- Devices. Only the host that took `disclose` knows the disclosure, so contacts that reach the user's other devices are
  not sent it.
- Resolution. Hosts parse references today; a shared resolver in the core would need the reference format specified here
  rather than by the publishing product.
