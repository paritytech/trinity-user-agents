---
title: "Personhood identity account"
owner: "@ERussel"
status: draft
---

# RFC 0028 — Personhood identity account

|                 |                                                                                                              |
| --------------- | ------------------------------------------------------------------------------------------------------------ |
| **RFC Number**  | 28                                                                                                           |
| **Start Date**  | 2026-11-07                                                                                                   |
| **Description** | Retire the `uid.<tld>` reserved product: `peopl.<tld>` account 0 is the wallet's main account and owns the person's lite and membership dotNS usernames. |
| **Authors**     | Ruslan Rezin                                                                                                     |

## Summary

The person's identity account becomes the personhood product's account 0, `//product//peopl.<tld>/index_bytes(0)`.
It owns both dotNS usernames, lite and membership. The `uid.<tld>` reserved product is
retired. This supersedes the `uid.<tld>` row of [RFC-0022](0022-account-derivations.md) built-in table.

## Definitions

- **Main account** — `//product//peopl.<tld>/index_bytes(0)`, the personhood product's account 0. **It is the one
  account that represents the person on chain and in the wallet:** the wallet shows it and spends from it; it owns
  the lite and membership usernames, so a payment addressed by username lands on it; it signs the lite reservation,
  the consumer registration and the `register_name` implication; it is the SSO statement key and the
  `identity_account_id` a paired host resolves a username for. Because it is Humanity's own account 0, Humanity signs as the name owner without any grant.
- **Personhood product** — `peopl.<tld>`, the reserved product id Humanity runs under on the network with TLD
  `<tld>`. Its ring-VRF domain holds the person keys; its account 0 is the main account.
- **Membership key** / **Devicehood key** — the ring-VRF keys `//peopl.<tld>//index_bytes(0)` and
  `//peopl.<tld>//index_bytes(1)`: the full person and the lite person. They live in a separate key tree and are
  not accounts.
- **Lite username** / **membership username** — `name.NN`, reserved through the attester's `reserve_name` and
  owned by the main account; and the bare `name`, registered with a link to the lite one.
- **Product account** — `//product//{productId}/{index}` per RFC-0022; `{index}` is a soft junction.

## Motivation

The personhood product (`peopl.<tld>`, Humanity) registers the membership username, and the lite username is the same
person's name linked to it on the gateway. The same product should manage both names, and one account should own them.
Today the names belong to `uid.<tld>` account 0, so Humanity can only sign as the owner through a `uid` context grant,
and two reserved products describe one person. The runtime already treats the owner as part of the personhood product:
every off-chain payload `uid/0` signs for the People chain (lite reservation, consumer registration, `register_person`)
is prefixed with the `peopl.<tld>` signing context, and every ring-VRF context it is used under is a `peopl.<tld>`
allocation. The account that signs `peopl` contexts should be a `peopl` account. Thus moving it makes the derivation match
what the chain already assumes.

## Approach

`peopl.<tld>` holds everything that identifies a person: its ring-VRF domain holds the devicehood and membership keys, 
  and its account 0 is the owner of both usernames. Hosts derive local activation, pairing attestation and the main account from it; the
lite-reservation candidate and the `who` of `register_name` are that account; username lookup resolves it. Polkadot App built-in product catalogues drop `uid`.

Humanity signs as itself, so the `uid` resolution and context-grant steps and the `uid` manifest go. A sign-up from
another product (for example, Jollity) needs a `peopl` context grant. Runtime, dotNS
contracts and attester do not change: `reserve_name` accepts any candidate and `register_name` with
`Link::LiteUsername` only requires `LiteLabelOwner == who`, which one owner satisfies.

## Trade-offs

- A product holding `peopl`'s context grant can act as the name owner and the main account.
  Accepted: Humanity is first-party and `peopl` is already a trusted label.
- Linkability. Product accounts hang off `//product//peopl.<tld>` by soft derivation, so whoever holds that subtree
  public key (a host paired to Humanity obtains it through `getProductSubtreePublicKey`) can compute every `peopl`
  account's public key. Today that links Humanity's accounts to each other; after this change it also links them to
  the wallet's main account. Secrets are not exposed, and the main account's address is already public as the name
  owner, but the wallet behind any `peopl` product account becomes identifiable to subtree-key holders. Accepted.
- No `uid` fallback and no migration. The derivation has not shipped to production; test identities re-attest.
- Dropped: lite name on a devicehood account, membership name on `peopl/0`. There is no devicehood account, and the
  gateway requires one owner for a linked registration; two owners need a gateway change and lose the shared chat key.
