---
title: "Honour voting proofs"
owner: "@illegalcall"
status: draft
---

# RFC: Honour voting proofs

## Summary

Add `account_create_honour_proof` to produce one Bandersnatch ring proof over
Honour's subject and point contexts using a registered full People key.
Products can use the result to authorize `Honour.bestow` on the existing runtime.

## Motivation

`account_create_account_proof` accepts one `ProductProofContext`.
[`Honour.bestow`](https://github.com/paritytech/individuality-community/blob/main/pallets/honour/src/types.rs)
requires two contexts in one proof; separate proofs cannot be combined.
Our workaround uses a patched test Host and a script. Its raw-context
development method is unavailable to production products.

## Requirements

- **Proof:** One proof binds the subject context, then the point context, to the
  message verified by `HonourAuth`.
- **Authorization:** The signing Host retains the private key and enforces the
  registered-ring and foreign-key permission rules from [RFC 0024](0024-personhood-as-product.md).
- **Compatibility:** Existing aliases, point allocations, and per-subject vote
  limits remain valid.
- **Portability:** Local and paired Hosts use the same request and response;
  the operation is unavailable if either the product Host or its signing peer
  lacks support.

## Approach

The request carries a key handle, ring location, 32-byte subject ID, point index
(`u8`), and 32-byte message. Products discover the registered key for the
requested full People ring rather than assuming a product name or key index.
The Host checks registration and full People membership before proving.

The Host derives exactly the two contexts defined by
[`VoteData::get_contexts`](https://github.com/paritytech/individuality-community/blob/main/pallets/honour/src/types.rs#L146-L173),
in subject-then-point order, and uses the Rust multi-context proof implementation.
The response contains one proof, both contextual aliases in that order, and
the ring index and revision.

This operation is an explicit compatibility exception to RFC 0024's
product-scoped context construction. Only the Host-computed Honour pair is
accepted. Foreign-key use requires the key owner's manifest allowlist, with
no user-prompt fallback. The paired Mobile signing Host enforces that check
independently of Desktop.

The Product SDK builds the message from the inherited implication verified by
`HonourAuth` and constructs the General V5 transaction. The message is opaque
to the Host, so authorization relies on the key owner's trust in the caller.
The chain checks the proof against the submitted call and extensions.

Use additive TrUAPI and SSO messages, generate bindings from the Rust contract,
and expose a supported Product SDK wrapper. Before releasing the core, Host
artifacts, and SDK, verify chain acceptance of an Honour vote and rejection of
altered messages or reversed contexts. Also verify denied key access, missing
full membership, a Desktop-to-Mobile proof round trip, and graceful
unavailability with an older signing peer.

## Trade-offs

- The Host gains a pallet-specific operation. Generic cryptography stays
  internal; a public arbitrary-context API would require a broader ownership policy.
- Migrating Honour to product-scoped contexts would align with RFC 0024, but
  changes the aliases used by `Honour.Points` and `Honour.Votes`. That requires
  a coordinated state migration preserving point allocations and vote limits.
