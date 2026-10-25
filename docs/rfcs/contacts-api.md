---
title: "Contacts API"
owner: "@filippovecchiato"
status: draft
---

# RFC — Contacts API

## Summary

_How the implemented pieces fit together is in
[Contacts Pick, End to End](../design/contacts-pick-end-to-end.md)._

A product asks the Host to let the user pick one or more contacts. The Host renders the
picker from its contact directory and returns opaque handles, never the list, names or
accounts. The handle is not an address: the core resolves it when building a transaction.
Host-owned name labels let users recognize selected handles without sharing the names
or requiring a Profile photo.

## Motivation

Each user has a different alias and account per context, so no handle identifies a person across
products, and a contact list is how a user keeps that private notebook. Products cannot use any of it
today, so users paste raw keys. But "send this NFT to a friend" needs one recipient the user chose,
not the address book — so this exposes the interaction rather than the list.

## Approach

Contacts come from the chat lists the Host's chat extensions hold; Hosts keep their own schema
and no new address book is imposed. A Host **renders the picker itself** and resolves handles on request, so names never cross to the product — which matters because a Host's only name for a contact
is often a globally correlatable People-chain username.

```rust
enum ContactPickOutcome {
  Picked { handle: [u8; 32] },
  Dismissed,
  NoContacts,
}

fn host_contacts_pick() -> Result<ContactPickOutcome, HostContactsPickError>;
```

Three outcomes because the retry decision differs: `Dismissed` is worth offering again, `NoContacts`
is not, and a Host with no picker answers `Unsupported`. No permission is requested — the user
selecting a contact is the consent.

The handle is one value per contact, the same in every product and on every Host of this user, keyed
on the user's entropy so no product can turn it back into an account. It is not an address: a product
names it as the recipient and the core substitutes the account when it builds the transaction. A
product-scoped address is not derivable at all, which is why the handle is resolvable rather than
directly usable.

### Multi-select audiences

Trait 20 method 0 remains `pick`. Method 1, `pickMany({ selected })`, edits a complete
selection of at most 256 handles. The core deduplicates and resolves the initial
selection before opening the picker; any unresolved handle rejects the whole request.
The host callback `pickContacts(product, ContactSelection { selected })` receives
accounts only inside the trusted host boundary. Confirming an empty selection returns
`Picked { handles: [] }`; closing the picker returns `Dismissed`. Session or directory
invalidation during resolution or confirmation cancels the change.

### Host-owned contact labels

Method 2, `placeLabels({ surfaceWidth, surfaceHeight, slots })`, replaces at most 256
name rectangles. Each slot supplies `{ slot, handle, rect, clip }`, reusing `AvatarRect`.
The host resolves handles and draws directory usernames, or account fallbacks, on its
own layer. Names do not depend on Profile disclosure. Missing contacts leave no label
and produce the same success response; products never receive names or availability.
Surfaces and rectangle sides are bounded to 16384 units, clip sides may be zero, and
slot ids must be unique. Empty slots, connection teardown and session changes clear
the layer. On same-wallet directory invalidation, the host clears stale names and
refreshes the latest live placement without another product request.


## Trade-offs

- A host that serves no picker answers `Unsupported`, which a product cannot retry its way out of.
- `NoContacts` reveals whether the user has any contacts — zero-or-not, never a count.
- No product-rendered contact directory: every selection is a host-owned user interaction.
- Dropped: returning the list scoped per product (`display_name` was a correlator no scoping fixed,
  and it needed a permission over the whole social graph); per-product handles (forfeit a durable
  shared id, break under contact sync); returning the chat account (transactable, but a global
  identifier any two products can join on); an unkeyed handle, or one keyed on the root account key
  (recoverable by hashing enumerable accounts).

## Substitution at signing

A product declares the handles its call names, on the transaction payload, and the Host replaces exactly those 32-byte runs with the accounts they resolve to. It declares them rather than passing an offset because an offset is a number the product computes about its own encoding and gets wrong silently, while a declared handle is either in the call or it is not: a Host that cannot find one refuses, rather than signing a call that names somebody else. A handle no contact matches refuses the same way, which is the only revocation this API has. The core sends the Host only the handles it has not cached, with the key they were minted under; the Host answers an account per handle and the core re-hashes each one, so a wrong answer refuses rather than pays. A Host empties the cache by signalling that its contacts changed. The signed call returns to the product with the real account in it, so a call naming contacts always asks the user, even under an auto-signing grant; a handle in the call that is not declared refuses rather than pays an address nobody holds. `contacts` never crosses to the signing host: the pairing Host relays the substituted call in the existing SSO shape, so host-papp and deployed wallets are unaffected.

Substitution happens before the confirmation, so the signing overlay is drawn from a call that names an account the Host can put a name to. That is what closes the display gap for the flow that matters: a product renders a neutral chip, and the user sees who they are paying in trusted UI at the moment of consent.

## Recognition outside signing

A product holds only handles and reserves rectangles for `placeLabels`. The host
draws names in those rectangles without returning a global correlator. Profile avatar
slots remain separate and photo-only, so users can recognize a contact even when that
contact has never shared a profile.
