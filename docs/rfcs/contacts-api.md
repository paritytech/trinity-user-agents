---
title: "Contacts API"
owner: "@filippovecchiato"
status: draft
---

# RFC — Contacts API

## Summary

_How the implemented pieces fit together is in
[Contacts Pick, End to End](../design/contacts-pick-end-to-end.md)._

A product asks the Host to let the user pick a contact. The Host renders an overlay from its Chat
workers' chat lists, the user selects one person, and the product receives one opaque handle — never
the list, a name, or an account. The handle is not an address: the core resolves it when building a
transaction.

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

## Trade-offs

- A host that serves no picker answers `Unsupported`, which a product cannot retry its way out of.
- `NoContacts` reveals whether the user has any contacts — zero-or-not, never a count.
- No product-rendered contact UI, every selection is a user interaction, one contact per call,
  read-only.
- Dropped: returning the list scoped per product (`display_name` was a correlator no scoping fixed,
  and it needed a permission over the whole social graph); per-product handles (forfeit a durable
  shared id, break under contact sync); returning the chat account (transactable, but a global
  identifier any two products can join on); an unkeyed handle, or one keyed on the root account key
  (recoverable by hashing enumerable accounts).

## Substitution at signing

A product declares the handles its call names, on the transaction payload, and the Host replaces exactly those 32-byte runs with the accounts they resolve to. It declares them rather than passing an offset because an offset is a number the product computes about its own encoding and gets wrong silently, while a declared handle is either in the call or it is not: a Host that cannot find one refuses, rather than signing a call that names somebody else. A handle no contact matches refuses the same way, which is the only revocation this API has. The core sends the Host only the handles it has not cached, with the key they were minted under; the Host answers an account per handle and the core re-hashes each one, so a wrong answer refuses rather than pays. A Host empties the cache by signalling that its contacts changed. The signed call returns to the product with the real account in it, so a call naming contacts always asks the user, even under an auto-signing grant; a handle in the call that is not declared refuses rather than pays an address nobody holds. `contacts` never crosses to the signing host: the pairing Host relays the substituted call in the existing SSO shape, so host-papp and deployed wallets are unaffected.

Substitution happens before the confirmation, so the signing overlay is drawn from a call that names an account the Host can put a name to. That is what closes the display gap for the flow that matters: a product renders a neutral chip, and the user sees who they are paying in trusted UI at the moment of consent.

## Open questions

How a product shows the user which contact they picked outside a signature. A product holds 32 bytes and no name, so it
renders a neutral chip. Two parts close that, and neither is specified here: the Host redraws the name
in its own signing confirmation, which knows the account and is where consent is given, so a product
never needs the name for the flow to be safe; and a product labels the handle itself, letting the user
name those 32 bytes once. A user-supplied label keeps the Host from handing back the correlator that
ruled out `display_name`.
