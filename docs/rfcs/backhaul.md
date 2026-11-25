---
title: "Backhaul: signaling protocol"
owner: "@ERussel"
status: draft
---

# RFC — Backhaul: signaling protocol

## Summary

Backhaul is the transport a product uses to reach another user agent: a product sends a signal
into a channel it holds a handle for, follows the signal to delivered or seen, and receives the
signals addressed to it. A channel connects two or more user agents over the chat protocol the
hosts already run, so a peer channel is the person chat with a contact and a group channel is a
group chat. The user agent owns channel setup, encryption, devices and compaction; the product
sees a handle, a signal id and a status. What a signal means is defined by the layer above, such
as Chat Extension v2, which carries its messages as signals.

## Motivation

- A product that wants to hand something to a contact, whether to another user or to the same
  product on that user's device, has no way to reach them.
- `contacts.pick` gives a product a handle for a contact, but the only thing a handle can do
  today is name a recipient inside a transaction.
- The chat between two people is run by the host and has no room for product content: its wire
  format carries text, media, reactions and calls, and custom content cannot leave the device.
- When the other user is not a contact yet, nothing can be sent before a chat request is
  accepted. A product has no access to the chat lifecycle, so it has no way to hold a message
  until the request is accepted.
- [Chat Extension v2](chat-modality.md) lets a product own rooms and push feed updates, but
  refuses a product that writes as the user into a room the host keeps, and a person chat is
  such a room. A product also cannot learn whether anything it sent arrived.

## Vocabulary

- **User agent (UA).** The host application of one user across their devices: it holds the
  user's keys, contacts and chats, and runs the core.
- **Product.** A dotNS-published application that runs inside the UA under its own id, as an
  app, a widget or a worker.
- **Signal.** One unit the backhaul carries from a UA to the other participants of a channel:
  a chat message of the base spec, or a product-defined payload.
- **Channel.** A set of UAs that exchange signals. A peer channel has two participants and is
  the person chat with a contact; a group channel has more.
- **Channel handle.** The opaque value a product holds for a channel. It is not an address and
  the product cannot turn it into one.
- **Compaction.** The existing mechanism of the chat protocol that moves undelivered messages
  that no longer fit a statement into one uploaded batch referenced by a single message.

## Requirements

- **Multiple channels.** Two UAs may exchange signals in more than one channel.
- **Group channels.** A channel may connect more than two UAs. A channel referenced by a
  contact handle is the peer channel with that contact.
- **Hidden signals.** An outside observer cannot read the content of a signal.
- **Channels exposed to products.** A product sends a signal into a channel and follows its
  status.
- **Setup abstracted.** A product that sends into a channel is never involved in the setup of
  the channel.
- **Hidden identity.** A product never learns a name, an account or a statement store topic; it
  holds the channel handle and nothing else. Different products receive different handles for
  the same channel. The UA can ask another participant of a channel for details such as its
  username.
- **Anonymity.** That a given set of UAs share a channel is not exposed to third parties.
- **Transport only.** The semantics of a signal, and the interfaces built on channels, are
  defined by other notions such as the chat extension.
- **Deferred delivery.** A signal sent into a channel that is not established yet waits, and
  goes as soon as the channel is.
- **Push notifications.** A signal may be shown in a push notification.
- **Multidevice abstraction.** The user's devices are not exposed to products. A product sends
  into a channel and the protocol delivers to every device of every participant.
- **Compaction.** A channel that overflows a statement uses the existing compaction mechanism,
  so it does not stall.

## Approach

The design has four parts:

- How a product obtains a channel handle.
- How a channel is established and how a signal travels in it.
- The product-defined signal and the status of a signal.
- The exchange of participant details inside a channel.

### Obtaining a channel handle

A product sends into a channel it holds a handle for. When it holds none, it asks the UA to
present an interface where the user selects the channel; the UA stores the association between
the selected channel and the handle it mints, and returns the handle. A product never sees the
list. Where the channel follows from the context, no interface is presented: a product opened
from a chat, for example the Scarcity card in a chat with a contact, receives the handle of that
chat.

For a peer channel the interface is the contact picker of the
[contacts RFC](contacts-api.md), extended with a dotNS username search so that a person who is
not a contact yet can be chosen. For a group channel the interface lists the group chats the
user is in, or lets the user compose one.

### Establishing a channel and carrying a signal

A channel that does not exist yet is established by the UA, never by the product. For a peer
channel the UA sends the chat request of the base specification; for a group channel the
request is extended to carry the group: its participants and a symmetric channel key the
creator generates and seals to each participant's identity chat key. Removing a participant
rotates the key. An MLS-based scheme is an alternative with the same shape for the product.

Every signal sent into a channel that is not established yet is kept by the UA with status
`new` and goes once the channel is. A signal's format, transport, encryption, multidevice
fan-out and compaction are those of the base specification and its multidevice extension: a
product signal is one more content variant of a message, so push notification, device sync and
compaction treat it as any message.

### The product signal and its status

A product-defined signal is a content variant of the base specification's message. The UA stamps
the product id from the calling execution; the receiving UA treats it as sender-asserted and
uses it only to choose which product reads the signal and draws it. A product signal carries a
one-line preview the UA shows wherever it cannot draw the signal, such as a push notification or
the chat list of a UA that lacks the product.

```rust
/// Transport between user agents, seen by a product as channels and signals.
///
/// Every method needs a session and a host that serves the backhaul;
/// otherwise it answers `NotConnected` or `Unsupported` as `contacts.pick`
/// does. There is no execution restriction: an app and a worker of the same
/// product see the same channels.
#[wire_trait(id = 27)]
#[async_trait::async_trait]
pub trait Backhaul: Send + Sync {
    /// Ask the UA to let the user select a channel. The answer is a handle
    /// this product keeps, or why nothing was selected.
    #[wire(id = 0)]
    async fn pick(
        &self,
        _cx: &CallContext,
        _request: HostBackhaulPickRequest,
    ) -> Result<HostBackhaulPickResponse, CallError<HostBackhaulPickError>> {
        Err(CallError::unavailable())
    }

    /// Send one signal into a channel. Returns the signal id the statuses
    /// are keyed on. `UnknownChannel` when the handle names no channel the
    /// UA holds; `SignalTooLarge` over the payload cap.
    #[wire(id = 1)]
    async fn send(
        &self,
        _cx: &CallContext,
        _request: HostBackhaulSendRequest,
    ) -> Result<HostBackhaulSendResponse, CallError<HostBackhaulSendError>> {
        Err(CallError::unavailable())
    }

    /// The events of a channel for this product: the current status of each
    /// signal it sent, the signals it received and has not acknowledged, then
    /// every later change.
    #[wire(id = 2)]
    async fn subscribe(
        &self,
        _cx: &CallContext,
        _request: HostBackhaulSubscribeRequest,
    ) -> Subscription<HostBackhaulSubscribeItem, CallError<HostBackhaulSubscribeError>> {
        Subscription::interrupted(CallError::unavailable())
    }

    /// Drop received signals the product has acted on. Idempotent.
    #[wire(id = 3)]
    async fn acknowledge(
        &self,
        _cx: &CallContext,
        _request: HostBackhaulAcknowledgeRequest,
    ) -> Result<HostBackhaulAcknowledgeResponse, CallError<HostBackhaulAcknowledgeError>> {
        Err(CallError::unavailable())
    }
}
```

```rust
/// Opaque per product: the same channel gives two products two handles.
pub struct ChannelHandle { pub bytes: Bytes32 }

/// Unique in its channel, minted by the UA. Also the message id of the
/// base specification.
pub type SignalId = String;

/// What a signal carries. The chat variants are those of the base
/// specification's message content; `product` and the details exchange are
/// the additions of the backhaul.
pub enum SignalContent {
    /// A message of the base specification, as the chat extension carries it.
    Chat(ChatMessageBody),
    /// A payload only the named product reads.
    Product(ProductSignal),
    /// A question about a participant, sealed to the channel (see below).
    DetailsRequest(SealedDetails),
    /// The answer, sealed to the asker.
    DetailsResponse(SealedDetails),
}

/// A product-defined signal.
pub struct ProductSignal {
    /// The product that reads the payload. Stamped by the sending UA.
    pub product_id: ProductId,
    /// Opaque to every UA. Bounded by the payload cap.
    pub payload: Vec<u8>,
    /// A signal this one relates to, such as the one it answers.
    pub related_to: Option<SignalId>,
    /// One line for a push notification and for a UA that cannot draw the
    /// signal. `None` leaves the UA its own placeholder.
    pub preview: Option<String>,
}

/// Where a sent signal is. Ranks are monotonic in declaration order.
pub enum OutgoingSignalStatus {
    /// Kept by the UA; the channel may not be established yet.
    New,
    /// Submitted to the statement store.
    Sent,
    /// Acknowledged by a device of every other participant.
    Delivered,
    /// Shown to every other participant, where their UAs send receipts.
    Seen,
    /// Will never be delivered.
    Failed,
}

/// One change in a channel.
pub enum SignalEvent {
    /// The status of signals this product sent.
    OutgoingStatuses(Vec<(SignalId, OutgoingSignalStatus)>),
    /// Signals addressed to this product.
    IncomingSignals(Vec<IncomingSignal>),
}

/// A received signal.
pub struct IncomingSignal {
    pub id: SignalId,
    /// The handle of the participant that sent it, in this product's handle
    /// space.
    pub from: ChannelHandle,
    pub content: SignalContent,
    /// Milliseconds since the Unix epoch, as the sender stamped it.
    pub timestamp: u64,
}

pub struct HostBackhaulSendRequest { pub channel: ChannelHandle, pub content: SignalContent }
pub struct HostBackhaulSendResponse { pub signal_id: SignalId }
pub struct HostBackhaulSubscribeRequest { pub channel: ChannelHandle }
pub struct HostBackhaulSubscribeItem { pub event: SignalEvent }
pub struct HostBackhaulAcknowledgeRequest { pub signal_ids: Vec<SignalId> }
```

`Sent` is the submission of the statement; `Delivered` is the transport acknowledgement of the
base specification; `Seen` is a receipt the receiving UA sends only for product signals and only
when its user allows receipts. In a group channel `Delivered` and `Seen` mean every other
participant.

### Exchanging participant details

A UA can ask another participant of a channel for details, such as its username, through a
signal in that channel, and shares its own details in the request. The request and the response
are sealed to the recipient, so the other participants of a group channel cannot read them. This
requires every participant of a group channel to publish a public key for the channel, which the
group request carries. A product never sees the details: the UA uses them to name the
participant in its own interface, such as a group chat where members are known only by their
channel keys.

```rust
/// Sealed to one participant's channel public key.
pub struct SealedDetails {
    /// The recipient's key, so the recipient knows which private key opens it.
    pub to: PublicKey,
    /// `DetailsRequest` or `DetailsResponse`, encrypted.
    pub ciphertext: Vec<u8>,
}

pub enum DetailsRequest { Username(Username) }
pub enum DetailsResponse { Username(Username) }

/// An X25519 public key, 32 bytes.
pub type PublicKey = [u8; 32];
```

## Use cases

- **Gifting an NFT.** Scarcity asks for a handle; the user picks a contact or searches a
  username. Scarcity creates a cheque sealed to the contact and sends it as a `Product` signal
  with the preview "Sent you an NFT". The signal waits behind the chat request if the person
  was not a contact, then shows in the chat as a card Scarcity draws. Scarcity on the other
  side reads the signal, deposits the cheque and acknowledges it. The sender follows the signal
  to `Delivered`, and to `Seen` where receipts are on.
- **A product message inside a chat.** The user is in a chat and opens a product from it. The
  product receives the chat's handle with no picker and sends into it directly.
- **A group chat.** A product asks for a group channel; the user picks a group or composes one.
  The UA establishes the channel with a sealed group key. Each UA asks the others for their
  usernames through the details exchange and names them in its chat list.
- **A product talking to itself.** The same product on two UAs exchanges `Product` signals in a
  channel the user opened once, such as a game sending its moves.

## Trade-offs

- A payload cap for a product signal, because the bytes ride inside a statement the hosts cap per
  batch; the code that reads them is not in the signal, so the cap is small.
- A product signal with no product on the receiving UA shows only its preview, with an action to
  open the product; the payload waits for that product.
- Status is per sending device. Another device of the same user answers `UnknownSignal`.
- `Seen` depends on a receipt the UAs do not send yet; `Delivered` is the practical end state
  until they do.
- Group channels need a group request, a channel key and per-channel participant keys that the
  base specification does not define yet; peer channels need none of it, so they ship first.
- Dropped: a permission a product requests up front (coarser than the pick); carrying the
  product's code in the signal (an HTML engine per UA); a transport the core runs itself (a chat
  engine rewrite in every UA).

## Open questions

- Whether the handle of a peer channel is the contact handle of `contacts.pick`, which the
  contacts RFC keys per user so that every product gets the same value, or a handle minted per
  product as required above. A per-product handle needs a second mapping beside the contact
  handle cache and a rule for a product that holds both.
- Whether the key the UAs store for a contact is the identity chat key a cheque and a group key
  must be sealed to, or a per-device key. If per device, the UA resolves the identity key on
  chain from the account.
- Which scheme the group channel key uses: a creator-generated symmetric key rotated on removal,
  or MLS.
