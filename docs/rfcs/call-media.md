---
title: "Call media of chat rooms"
owner: "Leonardo Custodio"
status: draft
---

# RFC: Call media of chat rooms

## Summary

A chat product that runs a call in one of its rooms (RFC "Chat Extension v2", "Calls") asks its host to start the
audio, or the audio and the video, of the call. The host runs the media itself, with its own call engine and its own
call screen. The two hosts agree on the media through signals that the products carry in their own encrypted channel.
The product never touches the microphone, the camera or the media.

## Motivation

The call slice of Chat Extension v2 gives a product the call buttons, the ring and the call bar, but no media. The
Media service that would carry the media of products does not exist yet. The native chat of the app already has a call
engine: WebRTC, the TURN relays of the backend, and the call screen. A product only needs a way to start it and to carry
its signals to the other side.

## Approach

A new `CallMedia` trait has three methods, and the chat room events get one more event.

```rust
pub struct HostCallMediaStartRequest {
    pub room_id: String,
    /// At most 128 bytes.
    pub call_id: String,
    pub video: bool,
    /// This side makes the offer. The side that accepted the call makes it.
    pub offerer: bool,
}

pub struct HostCallMediaSignalRequest {
    pub room_id: String,
    pub call_id: String,
    /// A signal of the host of the contact. At most 64 KiB.
    pub signal: Vec<u8>,
}

pub struct HostCallMediaStopRequest { pub room_id: String, pub call_id: String }

pub struct HostCallMediaResponse {}

pub enum HostCallMediaError {
    /// The user did not press a call button or accept a call in this room in the last 180 seconds.
    NotAllowed,
    InvalidRequest { reason: String },
    Unknown { reason: String },
}

#[wire_trait(id = 29)]
pub trait CallMedia {
    #[wire(id = 0)]
    async fn start(&self, cx: &CallContext, request: HostCallMediaStartRequest)
        -> Result<HostCallMediaResponse, CallError<HostCallMediaError>>;
    #[wire(id = 1)]
    async fn signal(&self, cx: &CallContext, request: HostCallMediaSignalRequest)
        -> Result<HostCallMediaResponse, CallError<HostCallMediaError>>;
    #[wire(id = 2)]
    async fn stop(&self, cx: &CallContext, request: HostCallMediaStopRequest)
        -> Result<HostCallMediaResponse, CallError<HostCallMediaError>>;
}

pub enum ChatRoomEvent {
    // ...
    /// A media signal of this host for the call. The product sends it to the contact.
    CallSignal { call_id: String, signal: Vec<u8> },
}
```

**The flow.**

1. The caller presses a call button (`CallRequested`). The product rings the contact through its own protocol.
2. The contact presses Accept (`CallAnswered { accept: true }`). Its product calls `start` with `offerer: true`.
3. The product of the caller learns of the acceptance and calls `start` with `offerer: false`.
4. Each host gives its signals to its product as `CallSignal` room events. Each product sends them to the contact,
   and the product of the contact gives them to its host with `signal`. A signal can come before its call starts; the
   host keeps it until the start.
5. End on the call bar, or the end of the call by the contact, makes each product call `stop`. A user who ends the
   call on the call screen of the host makes the host give a last `CallSignal` and a `CallEnded` room event.

**Consent.** The core allows `start` only within 180 seconds after `CallRequested` or an accepting `CallAnswered` in
the same room, the events that the host itself gives for a press of the user. A product cannot open the microphone
or the camera without a press. `signal` and `stop` need no press: without a started call they reach nothing.

**The signals are opaque.** The product does not read them. Their content belongs to the call engine of the host, so
both sides must run hosts with the same engine. The product carries them in its own end-to-end channel, so the
media agreement is as private as the messages.

| Answer        | What the product does next                                                 |
| ------------- | -------------------------------------------------------------------------- |
| `NotAllowed`  | Tells the user that the call has no media. The press is too old.           |
| `Unsupported` | Tells the user that the call has no media. This host has no call media.    |

**Hosts.** The iOS app runs the call engine and the call screen of its native chat, with the TURN relays of the
backend. The side that does not make the offer answers at once on the call screen, since the user accepted in the
room already. Android, the web hosts and the CLI host roles do not install call media yet and answer `Unsupported`.

## Trade-offs

- The media is the host's, not the Media service's. When that service exists, `start` can return a session of it,
  and products need no change.
- The engines of the two hosts must agree on the signal format. A host with another engine must refuse or translate.
- A call that rings while the app is closed (VoIP push) is out of scope. The ring needs the product to run.
