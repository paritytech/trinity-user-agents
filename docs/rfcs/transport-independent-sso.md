---
title: "Transport-independent SSO with opportunistic direct channels"
owner: "Karim"
status: draft
---

# RFC: Transport-independent SSO with opportunistic direct channels

## Summary

Hosts currently spend scarce Statement Store allowance on routine SSO traffic. As more hosts pair up, Those sessions compete for finite slots even when the two devices could reach each other directly.

SSO should separate its authenticated message protocol from its delivery transport. A session should prefer a direct channel when topology and device capabilities allow one, and retain Statement Store for signalling, offline delivery, wake-up and fallback.

## Motivation

Pairing and every later Accounts Protocol request currently travel through Statement Store. This gives the session a durable asynchronous channel, but also makes a chain resource the default path for two devices on the same network. It adds latency, depends on chain connectivity and consumes allowance that products need for their own statements.

RFC 0010 already names data channels and HOP as future Accounts Protocol transports, but leaves them out of scope. This RFC makes that transport boundary explicit.

## Approach

Keep the existing QR/deeplink bootstrap, signed messages, end-to-end encryption and message ids. Extend the pairing proposal so peers can advertise transport capabilities and negotiate the best mutually supported path:

- a WebRTC data channel for browser-compatible peer-to-peer communication;
- a local-network channel for native hosts on the same LAN;
- future nearby transports, such as native Bluetooth or direct Camera for Vault-like usage, where both platforms support them;
- Statement Store, which remains universally available as the durable fallback.

Transport negotiation is authenticated as part of the pairing transcript. A direct channel carries the same encrypted and signed SSO envelopes as Statement Store, so the transport is never trusted with session contents or identity.

The peers may establish or replace a direct channel without replacing the SSO session. When it is unavailable, messages continue through Statement Store. Message ids provide deduplication if a retry crosses transports. Implementations may also use Statement Store to exchange WebRTC signalling while carrying subsequent traffic over the data channel.

This isn't much different from how Apple / Google implement cross-device Passkey auth. 

## Trade-offs

- Direct transports reduce allowance use and foreground latency, but add connection negotiation and reconnection state.
- Mobile background execution makes a direct channel unreliable, so Statement Store remains necessary for **subsequent steps after pairing.**
- LAN and WebRTC expose additional metadata and platform permission surfaces even though message contents remain end-to-end encrypted.
- Delivery can race across transports, so replay protection, ordering and deduplication must be transport-independent.


