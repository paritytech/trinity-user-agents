---
title: "Bulletin allowance target and auto-renew"
owner: "Karol Kokoszka"
status: draft
---

# RFC — Bulletin allowance target and auto-renew

## Summary

A product sets a target: an amount of Bulletin bytes and transactions per 14-day People period. The
[Account Holder][terms] claims that amount for the product's [allowance account][terms] in the current and the next
People period, so that an authorization is active whenever Bulletin renews the product's data. The product reads the
target and its authorizations without contacting the Account Holder. New `Preimage` methods turn auto-renew on or off
for data on Bulletin, read how long Bulletin stores it, and list the data for which the product turned auto-renew on.

## Motivation

Bulletin stores data for a retention period. Auto-renew extends it: Bulletin charges each renewal to the current
authorization of the account that turned auto-renew on, and when that charge fails, it removes the registration and the
data expires.

A product doesn't have a TrUAPI call to turn auto-renew on, to request an amount per People period, or to read its
authorization. The product's allowance account receives authorizations from long-term storage claims on the People
chain. A person has 100 claims in each People period, and an authorization lasts 14 days from its first claim. The
Account Holder claims only when a product submits data or allocates a resource, and truapi's Account Holder and the iOS
app don't renew Bulletin allowances. A renewal that runs after the authorization ends and before the product's next
claim therefore fails.

Requirements:

- **Offline.** While the Account Holder is unreachable, a product reads its Bulletin allowance and the time it ends.
- **Consent.** The person approves every increase in the claims for a product per People period.
- **Idempotence.** When a product requests an amount that the person already approved, the request doesn't cause a
  prompt or an extra claim.
- **Separation.** Each product has a separate allowance account, and a call doesn't reveal what another product
  requested.
- **Compatibility.** A peer that doesn't support a new variant or method produces a defined result within the
  existing timeouts.

## Approach

The design depends on two runtime changes: the People chain accepts a long-term storage claim for the next People
period, and Bulletin stores one authorization per account and People period, which covers exactly that period.

The design has five parts:

- A target resource and a `state` method in `ResourceAllocation`.
- Auto-renew methods in `Preimage`.
- On the [Host][terms], a record of the last target it sent for each product and the Account Holder's answer to it.
- On the Account Holder, a record of each product's target, and a renewal pass that claims up to that target.
- Changes to [RFC-0010][rfc-0010].

### ResourceAllocation

- RFC-0010's `AllocatableResource` has a new variant, `BulletinAllowanceTarget`, which sets the product's target in a
  `request` call. A target of zero stops automatic claims.
- A new `state` method returns the product's Bulletin state: the last target the Host sent and the answer to it, the
  caps and usage of the active authorization, and the end of the last People period that has an authorization. The
  Host reads the values from its own record and from Bulletin, and doesn't contact the Account Holder.
- `AllocationOutcome` has a new variant, `PendingApproval`, for target requests, and `Allocated` contains the Bulletin
  state for `BulletinAllowance` and target requests.
- The request and response have `V2` variants that contain the new resource and outcomes. A product that sends `V1`
  receives the RFC-0010 outcomes.
- A higher target adds claims to the authorizations of the current and the next People period. A lower target applies
  from the first People period that the Account Holder hasn't claimed yet, because claims that it already made remain
  valid.

What a target request returns:

| Case | Outcome |
| --- | --- |
| The Account Holder recorded the target, and the authorizations of the current and the next People period cover it | `Allocated` |
| The Account Holder recorded the target, and the person doesn't have enough free claims | `Allocated`, with `NoMoreClaims` in the state |
| The Account Holder recorded the target, and a claim failed | `Allocated`, with `ClaimFailure` in the state |
| The person declined the target | `Rejected`, and the previous target remains active |
| The Account Holder's answer hasn't arrived | `PendingApproval` |
| The Host or the Account Holder doesn't support targets | `NotAvailable` |

### Preimage

New `Preimage` methods:

| Method | What it does |
| --- | --- |
| `set_auto_renew` | Turns Bulletin auto-renew on or off for the data under a key, in a transaction from the product's allowance account |
| `status` | Returns the size of the data under a key, the time until which Bulletin stores it, and its auto-renew registration |
| `list` | Returns the keys whose auto-renew registration belongs to the product's allowance account |

- Turning auto-renew on charges the first renewal to the active authorization, and Bulletin rejects turning it off
  until that renewal has run.
- `set_auto_renew` requires the `PreimageSubmit` permission, submits a transaction only when the registration differs
  from the requested state, and never claims.

### Host

- The Host stores, per product, the last target it sent and the answer to it, next to the product's allowance key. The
  record contains `PendingApproval` until the answer arrives.
- `request` sends the target to the Account Holder in these cases, and otherwise returns the answer in the record:
  - The record doesn't contain the same target.
  - The answer in the record is `NoMoreClaims` or `ClaimFailure` from an earlier People period.
  - The answer in the record is `PendingApproval`, and the Accounts Protocol session doesn't contain a pending request
    for the target, for example after a newer request replaced it.
- The Host sends a target as the only resource of an Accounts Protocol request with `on_existing: Ignore`, and doesn't
  show its own confirmation for it. `on_existing` doesn't apply to a target.
- When the Account Holder's answer doesn't arrive within the Host's timeout, `request` returns `PendingApproval`. The
  Host doesn't withdraw the request, and records the answer and the allowance key when they arrive.

### Account Holder

- The Account Holder stores one target record per product on the device and excludes it from backup.
- The Account Holder prompts the person for a target with more bytes or transactions than the target in the record,
  and records the same or a lower target without a prompt.
- The renewal pass makes, for each record, the claims that bring the authorizations of the current and the next People
  period up to the target.
- The Account Holder runs the pass when a target request arrives and with its other allowance renewals.
- The Account Holder claims with the person's `People` (full-person) membership, and with the `LitePeople` membership
  only when the person doesn't have a `People` membership.
- An Account Holder without target support either acknowledges the request and doesn't send an answer, so `request`
  returns `PendingApproval`, or acknowledges it with `DecodingFailed`, so `request` returns `NotAvailable`.

### Changes to RFC-0010

- `ApAllocatableResource` has a new variant, `BulletinAllowanceTarget`, with the amount. `ApAllocatedResource` has a
  new variant, `BulletinAllowanceTarget`, which contains the allowance key, and `NoMoreClaims` or `ClaimFailure` when
  the Account Holder couldn't claim the full target.
- The Account Holder renews a Bulletin allowance only for a product with a target, which replaces the rule "Renewal is
  not a product concern" for Bulletin.
- The Account Holder claims long-term storage for the current and the next People period.
- Long-term storage claims are counters within a People period, and a new claim doesn't evict an older one. The answer
  to a target request contains `NoMoreClaims` when the period doesn't have a free counter, in place of RFC-0010's
  eviction prompt.

## Trade-offs

- Each Host stores its own record, so two Hosts, or one Host after the person clears its site data, can report different
  targets for a product. The Account Holder claims according to its own record.
- The Host sends a target in a separate request, which costs a second round trip when a product also allocates other
  resources.
- `set_auto_renew` never claims, so a product without an active authorization sets a target first.
- Bulletin doesn't index auto-renew registrations by account, so `list` reads all of them.
- The Host receives the reason for a failed pass only with the answer to the next request, so `state` can show an
  ended authorization without a reason.
- The Account Holder's app has to run at least once in every People period. If it doesn't run during a period, the
  Account Holder doesn't claim the period after it, and every renewal in that period fails.

## Open questions

- Host confirmation: should the Host confirm a target before it sends it? Without a Host confirmation, the person
  approves a target only on the Account Holder.
- Restoring targets: an Account Holder on a new or reset phone doesn't have the target records, and a Host doesn't send
  a target again after an `Allocated` answer without a reason, so automatic claims stop until the product changes its
  target.
- Wait time: `@parity/truapi`, the TypeScript library that products use to call TrUAPI, fails a request after 120 s by
  default, and the Host waits up to 300 s for the Account Holder. A product with the default timeout therefore receives
  a timeout error instead of `PendingApproval`. Because the Host records a late answer to a target, it could return
  `PendingApproval` before the 120 s timeout.