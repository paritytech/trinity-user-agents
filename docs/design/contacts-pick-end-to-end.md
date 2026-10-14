---
title: "Contacts Pick, End to End"
type: design
status: draft
---

# Contacts Pick, End to End

_Traces one contact selection through every layer. The design rationale is in
[RFC — Contacts API](../rfcs/contacts-api.md); this is how the pieces fit._

## Summary

A product asks for a contact and gets back 32 bytes. The host draws the picker, so the names stay
host-side; the core turns the chosen account into a **handle**, a keyed hash of that account under
the user's own entropy. The handle is not an address and cannot be turned into one.

To pay that person, the product puts the handle where the recipient goes in its call and declares it
on the transaction payload. The core swaps those 32 bytes for the real account before the call is
shown or signed. So the product names a recipient it never learns the account of, and the user sees
who they are paying in trusted UI.

Two halves, and both are needed: **pick** mints the handle, **create_transaction** redeems it.

## Who holds what

```
product         handle (32 bytes)                        — never a name, account, or list
  │
  │ contacts.pick
  ▼
core            handle_key = f(root entropy source)      — mints handles, checks answers
  │             handle → account cache
  │ ContactsPlatform::{contacts(lookup), pick_contact}
  ▼
host            the contact list, the names, the picker UI
```

The `handle_key` is derived from the user's root entropy source, not from an account key, so a
handle cannot be recovered by hashing enumerable accounts. One contact has one handle across every
product and every host of this user, and two users have different handles for the same contact.

## Half one — pick

1. Product calls `contacts.pick`.
2. Core checks a session exists and a contacts platform is installed. Neither means `Unsupported`
   or `NotConnected`, answered without an overlay ever being raised. There is no permission step:
   the user selecting a contact **is** the consent.
3. Core calls `ContactsPlatform::pick_contact(product_id)`.
4. Host draws its own picker over whatever is on screen, naming the requesting product.
5. User taps someone. The host answers `Picked { account }`, `Dismissed`, or `Unsupported`.
6. Core hashes the account under `handle_key` and returns `Picked { handle }`.

The product-facing outcome is `Picked / Dismissed / NoContacts`; `Unsupported` is a call error, not
an outcome. The three are distinct because the retry decision differs — `Dismissed` is worth
offering again, `NoContacts` is not, and a host with no picker will never have one.

## Half two — redeem

1. Product builds its own call with the handle where the recipient goes, and sets
   `contacts: [handle]` on `ProductAccountTxPayload`.
2. `create_transaction` resolves each declared handle from the core's cache of handles it minted or
   resolved. The rest go to the host in one `contacts(lookup)` call carrying the handle key; the host
   answers an account or `None` per handle, and the core re-hashes every account it gets back,
   refusing one that does not match. A forged handle is never cached, so it always meets the host
   and the check. The host calls `notify_contacts_changed()` when a contact is removed or blocked,
   which empties the cache, so a removed contact stops resolving.
3. Substitution replaces **exactly** those declared 32-byte runs. Anything undeclared is left alone.
4. The substituted call goes to the confirmation *and* to the signer, in that order. A call naming
   contacts is always confirmed, even under an auto-signing grant, because the signed call the
   product gets back carries the real account.

A handle is declared rather than located by offset because an offset is a number the product
computes about its own encoding and gets wrong silently, while a declared handle is either in the
call or it is not. Both failures refuse the whole transaction with `UnknownContact`:

- the handle names nobody the host has a contact for — a stale handle and a forged one look the
  same, which is the only revocation this API has;
- the handle is not in the call the product said it was in.

Substituting before the confirmation is what closes the display gap: the product renders a neutral
chip, and the host's signing overlay knows the account and can put a name to it at the moment of
consent.

Only `create_transaction` substitutes. `sign_payload` and `sign_raw` take pre-encoded payloads with
no `contacts` field, so a handle in one of those is signed as-is.

## Where it lives

| Layer | Path |
| --- | --- |
| Wire trait | `rust/crates/truapi/src/api/contacts.rs` |
| `ContactHandle`, `ContactPickOutcome` | `rust/crates/truapi/src/v01/contacts.rs` |
| `contacts` on the tx payload | `rust/crates/truapi/src/v01/transaction.rs` |
| Host syscall trait | `rust/crates/truapi/src/platform.rs` (`ContactsPlatform`) |
| `pick` handler, resolution | `rust/crates/truapi/src/runtime.rs` |
| Minting and resolving | `rust/crates/truapi/src/runtime/contacts.rs` |
| Byte substitution | `rust/crates/truapi/src/host_logic/contact_substitution.rs` |
| Substitution call site | `rust/crates/truapi/src/runtime/capabilities/signing.rs` |
| Native callback boundary | `rust/crates/truapi/src/native/callbacks.rs` (`NativeContactsCallbacks`) |
| iOS bridge + picker | `hosts/ios/polkadot-app/Modules/Products/TrUAPI/AppContactsHostBridge.swift`, `Modules/Products/APContactPick/` |
| Android bridge + picker | `hosts/android/feature/products/impl/.../truapi/AppContactsHostBridge.kt`, `presentation/truapiContactPick/` |
| Headless host | `rust/crates/truapi-host-cli/src/contacts.rs` |

## Implementing it on a host

Serve `contacts(lookup)` and `pick_contact(product_id)`. Beyond that:

- **Hash the same way.** A contact's handle is BLAKE2b-256 keyed with `lookup.handle_key` over the
  contact's 32 raw account bytes. Key `0x11` × 32 and account `0x22` × 32 give
  `d48c96fce9805f689b0bfa602feacdf3c7770d27e76c25d980eff0955e3714d2`. The key is per session, so a host that indexes the hash recomputes it
  when the session changes.
- **Answer every handle, in order.** One entry per handle, `None` where no contact matches.
- **Answer `NoContacts` rather than draw an empty sheet.**

- **Drop blocked contacts.** Offering someone the user refused is worse than offering nobody.
- **Answer a dismissal.** The core blocks on the reply, so a picker the user swipes away must
  resolve as naming nobody rather than leaving the call outstanding.
- **Name the product.** The user is deciding who to tell, so the sheet says who is asking.
- **Signal removals.** Call `notify_contacts_changed()` whenever a contact is removed or blocked;
  without it, a handle the core cached keeps resolving until the runtime restarts.
- **Never send the list anywhere.** The lookup answers only the handles asked about.
