---
title: "Honour voting proofs"
owner: "@illegalcall"
status: draft
---

# RFC: Honour voting proofs

## Summary

Add `account_create_honour_proof` so products can request a proof for an Honour vote.
The Host uses a registered full People key to produce one Bandersnatch ring proof.
This proof covers both the subject context and the point context.
Products can use the proof to authorize `Honour.bestow` on the existing runtime.

## Motivation

`account_create_account_proof` accepts one `ProductProofContext`.
[`Honour.bestow`](https://github.com/paritytech/individuality-community/blob/main/pallets/honour/src/types.rs)
requires two contexts in one proof.
Products cannot combine two separate proofs to meet this requirement.

## Requirements

- **Proof:** One proof binds both contexts to the message that `HonourAuth` checks.
  The subject context comes first. The point context comes second.
- **Authorization:** The signing Host keeps the private key.
  It enforces the rules for registered rings and foreign keys in [RFC 0024](0024-personhood-as-product.md).
- **Compatibility:** Existing aliases, point allocations, and vote limits for each
  subject remain valid.
- **Portability:** Local and paired Hosts use the same request and response.
  The operation is unavailable if the product Host or its signing peer does not
  support it.

## Approach

The request contains:

- A key handle.
- A ring location.
- A 32-byte subject ID.
- A point index (`u8`).
- A 32-byte message.

Products find the registered key for the requested full People ring.
They do not assume a product name or key index.
Before it creates the proof, the Host checks key registration and full People membership.

The Host derives the two contexts defined by
[`VoteData::get_contexts`](https://github.com/paritytech/individuality-community/blob/main/pallets/honour/src/types.rs#L146-L173).
It places the subject context before the point context.
It creates the proof with the Rust implementation that supports multiple contexts.

The response contains one proof and both contextual aliases.
The subject alias comes first. The point alias comes second.
The response also contains the ring index and ring revision.

RFC 0024 normally requires products to use contexts scoped to the product.
This operation makes an explicit exception to preserve compatibility with Honour.
The Host accepts only the Honour context pair that it computes.

A product can use a foreign key only if the key owner's manifest allowlist permits that product.
User approval cannot replace this permission.
For paired requests, the Mobile signing Host checks this permission independently of Desktop.

The Product SDK builds the message from the inherited implication that `HonourAuth` checks.
It also constructs the General V5 transaction.
The Host treats the message as opaque data.
Therefore, authorization depends on the key owner's trust in the calling product.
The chain checks the proof against the submitted call and transaction extensions.

Add new TrUAPI and SSO messages without changing existing messages.
Generate the bindings from the Rust contract.
Provide a supported wrapper in the Product SDK.

Before releasing the core, Host artifacts, and SDK, check these cases:

- The chain accepts an Honour vote with a valid proof.
- The chain rejects a proof with a changed message.
- The chain rejects a proof with reversed contexts.
- The Host enforces denied key access.
- The Host handles missing full People membership.
- A Desktop request produces a proof through the paired Mobile signing Host.
- The operation reports that it is unavailable when an older signing peer lacks support.

## Trade-offs

- The Host gains an operation specific to the Honour pallet.
  Generic cryptography remains internal.
  A public API that accepts arbitrary contexts would require a broader policy for key ownership.
- Changing Honour to use contexts scoped to the product would align it with RFC 0024.
  However, it would change the aliases used by `Honour.Points` and `Honour.Votes`.
  It would require a coordinated migration of stored state.
  The migration must preserve point allocations and vote limits.
