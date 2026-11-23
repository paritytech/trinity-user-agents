---
title: "Backhaul: product messages to contacts"
owner: "@ERussel"
status: draft
---

# RFC — Backhaul: signaling protocol

## Summary

## Motivation

- A product that wants to hand something to a contact, another user or product, has no way to reach them;

- `contacts.pick` gives it a contact a handle a product can reference, but the only thing a handle can do today is name a recipient
inside a transaction;

- The chat between two people is run by the host and has no room for
product content: its wire format carries text, media, reactions and calls, and custom content
cannot leave the device;

- If another user is not in the contact list then there is no way to send a message waiting for the chat request beforehand. However a product has no access to the chat lifecycle and thus has no mechanism to postpone message sending till chat request is accepted;

- Chat Extension v2 lets a product own rooms and push feed updates, but
refuses a product that writes as the user into a room the host keeps, and a person chat is such
a room. A product also cannot learn whether anything it sent arrived.

## Vocabulary

- User Agent (UA)
- Product
- Signal
- Channel
- Compaction

## Requirements

- **Multiple channels.** Two UA MAY exchange signals in more then one channel.
- **Group channels.** Each channel MAY connect more then two UA. A channel referenced by contact handle is peer to peer channel with a given contact.
- **Signal is hidden.** External observer can't read content of the signal;
- **Channels are exposed to products.** A product send a signal to a channel and track it's status;
- **Channel negotiation is abstracted.** A product that wants to send a signal in a given channel SHOULD NOT be bothered with it's setup process;
- **Channel id is hidden.** A product never learns a name, an account or real statement store topic; it holds the
  channelHandle and nothing else. Different products receive different handles for the same channel. UA can request identity details of the another participant of the channel such as username.
- **Anonimity.** The fact that a given set of UA has a channel to exchange signals is not exposed to third parties;
- **Transport only.** Signal, channel semantics and interfaces are defined by other notions such as chat extension;
- **Push notifications support.** A signal MAY be displayed in push notification.
- **Multidevice abstraction.** User's devices are not exposed to the products. A product sends messages in the given channel and the protocol ensures they are delivered to all associated devices.
- **Compaction support.** Overflowed channel must utilize existing compaction mechanism to prevent channel stallness.

## Approach

1. Product sends a message

- Finds channelHandle. If product doesn't have it in the own storage yet it ask host to present an interface to the user to select the channel.
- Host stores association between selected channel and created handle. The handle returned to the product to reference channel later. Note that a user might be in a flow where channel can be derived from the context. In that case no interface is presented. For example, if a user already in the chat with another user and decides to send NFT then Scarcity gets current contact's handle without any additional interface to pick one.
- If channel doesn't exist then a user selects participants of the channel and initiates channel establishment procedure. For single peer channel extisting protocol (base spec) for chat request is used and for multipeer case the protocol is extended to carry group channel information.
- Any signal sent to the unestablished channel is pending until channel is established
- Signal format, trasport and signal's status logic is defined by base spec. For group channels a separate specification will be prepared: UA generates a symmetric key and shares it in the chat request with other participants, in case a participant should be removed the key is rotated (alternatively MLS based schemes are possible).
- A special signal case .product(payload, signalId?) is defined to allow products specify their custom messages
- A UA can request another UA's details via a separate signal via given channel. UA shares own details in the request as well. For example, this can be used to allow one UA to request another UA's username in the group chat. See DetailsRequest and DetailsResponse. This add a requirements to group channel that asks each participat to submit their public keys per channel.

The following interface available to products:

func send(SignalContent, ChannelHandle) -> SignalId
func subscribe(ChannelHandle) -> Stream<SignalEvent>

enum SignalEvent {
    outgoingStatuses([OutgoingSignalStatus])
    incomingSignals([IncomingSignal])
}

func acknowledge(Set<SignalId>)

SignalContent {
    // similar to RemoteMessage from base spec
    ...
    raw(Bytes)
    detailsRequest(EncryptedDetailsRequest, PublicKey)
    detailsResponse(EncryptedDetailsResponse, PublicKey)
}

struct IncomingSignal {
    id: SignalId
    content: SignalContent
}

enum OutgoingSignalStatus {
    case new
    case sent
    case delivered
    case seen
}

PublicKey = ECDH public key's byte array

struct DetailsRequest {
    case username(Username)
}

struct DetailsResponse {
    case username(Username)
}

## Use cases