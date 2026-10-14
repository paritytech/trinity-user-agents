---
title: "Core-resolved product manifests"
owner: "@filippovecchiato"
status: draft
---

# RFC — Core-resolved product manifests

## Summary

The core resolves each product's root manifest from dotNS and answers whether one
product grants another a scope. Hosts neither fetch manifests nor decide what a grant
covers.

## Motivation

[Scoped grants in `trustedProducts`][granted] defines what a publisher can pre-approve,
and the core already adjudicates every cross-product call in one place. But nothing
reads a manifest, so every grant is refused: a publisher who grants `storage` to a
partner still sees every read fail.

Leaving the reading to hosts scatters it. Each would resolve, parse and adjudicate on
its own, and they would disagree — as they already do for cross-product ring-VRF keys,
where one host prompts and the core refuses. A grant that means one thing on a phone and
another on a desktop is not one a publisher can reason about.

## Approach

The core performs the resolution [RFC — Product Manifest Format][manifest] already
specifies and answers grant questions from the result. Nothing about the format changes.

Every reason a grant cannot be established is one answer: unresolvable product, no
manifest, unparseable document, unreachable chain, narrower scope. Distinguishing them
would turn any cross-product call into a probe for which products exist. It also means
an unreachable chain withdraws grants rather than assuming them.

A resolved manifest is cached and honoured for one day. dotNS attaches no signal to a
record edit, so that lifetime is the only bound on a revoked grant — a security
parameter, which is why it is fixed here rather than left to each host.

## Trade-offs

- A revoked grant stays in force for up to a day. The alternative is a chain read on
  every cross-product call.
- A grant is only as strong as dotNS ownership: a transferred name widens access with
  one `setText`. Inherent to publisher-declared trust, and why a grant never waives a
  denial the user already gave.
- Hosts lose their own trust policy. That is the point, but a host with reason to be
  stricter has nowhere to express it.
- Dropped: resolving per host, which reproduces the divergence above; and caching until
  evicted, which makes revocation impossible.

[granted]: granted-scopes.md
[manifest]: product-manifest.md
