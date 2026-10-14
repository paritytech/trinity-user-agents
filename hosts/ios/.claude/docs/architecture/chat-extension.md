# Chat & Messaging Architecture

## Overview

Chat is a core feature composed of multiple sub-modules under `Modules/Chat/`. It supports real-time messaging via statement store, WebRTC voice/video calls, file attachments, and payment requests.

## Key Components

### Modules
- **Chat** — main chat interface (sub-modules for messages, input, media, etc.)
- **ChatCall** — voice/video call management
- **ChatRequestList** — pending chat requests
- **ChatAttachments** — file/media attachments
- **IncomingChatRequest** — incoming request handling
- **ContactsList** — contacts management
- **SearchContact** — contact search

### Packages
- **MessageExchangeKit** — encrypted messaging protocol layer
- **ChatStorage** — chat data persistence (CoreData)
- **StatementStore** — off-chain message storage and retrieval

### Services (in ServiceCoordinator)
- `chatCoordinator` — chat session management
- `callCoordinator` — WebRTC call management
- `attachmentUploadService` / `attachmentDownloadService` — file handling

> SSO/sign-in flows live outside chat. See `architecture/sso.md`.

## Architecture Patterns

### Message Flow
1. User composes message in Chat module
2. Message encrypted via MessageExchangeKit
3. Statement published to statement store (on-chain reference)
4. Recipient receives via statement store subscription
5. Message decrypted and displayed

### WebRTC Calls
- PushKit (VoIP) for incoming call notifications
- WebRTC for peer-to-peer audio/video
- `callCoordinator` manages call lifecycle
- See `architecture/data-transport.md` for transport layer details

### Push notification payload

After a message is posted to the statement store, the sender also pushes it through the relay
(`POST /api/v1/notify`) so the recipient's Notification Service Extension can render it without
opening a statement-store connection. The `message` field is `hex(ChaCha20-Poly1305(SCALE(payload)))`,
so hex doubles the size and an APNs alert (4 KB total) leaves under 2 KB of plaintext.

The payload is the push-only `Chat.NotificationPayload`: `messageId ‖ timestamp ‖ version u8 = 0 ‖ kind u8 ‖ content`, where
kind `0` is Stripped and `1` is Full. Full carries `RemoteMessageContentV1` unchanged. Stripped
(`Chat.StrippedContentV1`) is used when the full content exceeds 1800 bytes.

### Media attachment thumbnails

The optional `thumbnail: Data` field in image and video metadata contains a BlurHash string encoded as UTF-8. Senders generate the hash with 4×3 components from an image no larger than 128 points on its longest side. Receivers must parse the bytes through the typed `BlurHash` boundary before rendering. Invalid UTF-8, malformed BlurHash values, and legacy binary thumbnail bytes are treated as a missing preview; the full attachment download continues normally.

## Hard Rules

1. **Chat extensions must not block the main chat flow** — extensions are additive surfaces (payment requests). Coinage transfer bubbles are not an extension: they render `Content.Transfer.state`, read from the transfer state row related to the message (see architecture/coinage.md, "Chat transfer lifecycle"). Their lifecycle and failures must not stall message send/receive, scroll, or compose state. Concretely:
   - Extension setup is asynchronous and off the chat send path. Don't `await` extension readiness before letting the user type or send. A still-loading payment extension renders as a placeholder cell; the message above and below it still sends and renders.
   - Extension errors stay inside the extension. A failed `CoinagePaymentProcessingExtension` payment confirmation surfaces an in-cell error state — it does not throw out of the chat interactor or hide the underlying message.
   - Disable extensions at the registry/factory level (`ChatExtensionsRegistry.createDimExtensions`), never via `return nil` inside an extension method. A half-initialized extension that no-ops at runtime can still block the chat flow when other code awaits it.
2. **Message rendering state in dedicated handlers, not in main chat** — keeps chat module manageable
3. **Encryption is mandatory** — all messages go through MessageExchangeKit
4. **Persisted key ids**: `signKeyId` is a substrate derivation path (product-account ids use the hex index segment — build via `WalletDerivationPath`/`ProductDerivationPath`, never hand-format); `encryptionKeyId` is a `ChatEncryptionDomain` raw value, not a path.

## Anti-Patterns

| Anti-pattern                              | Do instead                                              |
|-------------------------------------------|---------------------------------------------------------|
| Awaiting extension readiness on the send path | Render placeholder cells; let send/receive proceed |
| Hardcoding message types                  | Use extensible message type system                      |
| Blocking UI during message send           | Optimistic UI update, handle failure async              |

## Mid-Migration Notes

- Chat is actively evolving with new extensions (payment requests)
- Statement store communication layer is the north-star for messaging
- Legacy patterns may exist in older sub-modules; new code follows current architecture
