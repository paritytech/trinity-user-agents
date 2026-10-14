# Message Compaction (Chat RFC-0002)

When the undelivered messages for a peer no longer fit the outstanding request statement
(~4 KB), the sender uploads them as one encrypted batch to HOP and replaces them with a single
fixed-size `compactedMessages` message (content index 19). The recipient claims, decrypts, and
expands the batch transparently. Reuses the RFC-0001 machinery end-to-end — see
[chat-attachments.md](chat-attachments.md).

## Component Map

| Component | Where | Role |
|---|---|---|
| `OutgoingChannelCompactionProxy` | `Packages/MessageExchangeKit` | Wraps the outgoing channel; probes encoded request size, triggers compaction, maps compacted ↔ original messages |
| `OutgoingRequestSizeValidator` | `Packages/MessageExchangeKit` | `maxPayloadSize = maxStatementSize − reserved (proof/expiry/topic)` |
| `MessageCompacting` / `AnyMessageCompactor` | `Packages/MessageExchangeKit` | Seam the app implements to produce the compacted message |
| `ChatMessageCompactor` (`ChatMessageCompactorFactory`) | app `Modules/Chat/Service/Compaction` | Encodes the batch, allocates allowance, uploads via `uploadBlob`, builds the `compactedMessages` reference |
| `CompactedMessageExpansionService` | app | Subscribes to `.incomingCompactedMessages()` CoreData snapshots, throttles concurrent expansions |
| `ChatMessageClaimer` | app | Trusted-node check, `downloadBlob`, decodes the batch, hands messages to expansion |
| `CompactionCommitMapper` / `CompactedExpansionMessageMapper` | app | Partial CoreData mappers: mark originals with `compactionId`; persist expanded messages + set `contentExpanded` |
| `uploadBlob` / `downloadBlob` on `HandoffFileLoader` | `Packages/HandoffService` | Envelope-aware, inline-only blob path (see Wire Format) |

## Wire Format

- Batch plaintext = bare SCALE `[EncodedMessage]` (each element the opaque per-message
  encoding, i.e. `Chat.OpaqueMessage`). **No compaction-specific container or version
  prefix** — RFC-0002 explicitly forbids one; versioning comes from the pool envelope and each
  message's own versioned content. Pinned by `CompactedBatchCodingTests`.
- Pool entry = `VersionedUploadedFile.v1(inline(batchBytes))`, encrypted with the
  ticket-derived AES key — a standard RFC-0001 inline entry. Pinned by
  `HandoffFileLoaderBlobTests`.
- Reference = `CompactedMessagesContent { claimIdentifier, claimTicket, node }`
  (`P2PMixnetFile` minus `meta`).

## Invariants

- **Inline-only, both sides.** `uploadBlob` throws `inlineLimitExceeded` above
  `config.inlineThreshold`; `downloadBlob` rejects a `chunked` payload as malformed
  (`unexpectedChunkedPayload`) without fetching any chunks. This stops compaction from
  becoming an unbounded transport — bulk data belongs in attachments.
- **Confirm before ack.** `downloadBlob` runs `onConfirm` (durable persistence via
  `CompactedExpansionMessageMapper`) before `hop_ack`; the ticket-derived key is the sole
  recipient, so an early ack risks unrecoverable loss.
- **Trusted-node check is mandatory** (`HOPNodeProviding.isNodeAllowed`) before any claim.
- **Compacted messages are transport containers** — never displayed; fresh `messageId` and
  compaction-time timestamp; contained messages keep their original ids/timestamps so dedup
  and ordering are unaffected.
- **Recursive expansion is emergent:** expanded nested `compactedMessages` persist with
  `contentExpanded == NO` and re-enter the `.incomingCompactedMessages()` subscription.
- **Delegate ordering contract:** `didCompactMessages` fires before the compacted message
  enters the underlying channel; the DB commit is async behind the submission round-trip.
- Compacted entries claim through `HandoffServiceDecorator`, so the RFC-0001 bitswap fallback
  (and its skip-ack-for-chain-sourced rule) applies unchanged.

## Persistence (UserDataModel42)

`CDChatMessage.compactionId` links originals to their compacted container;
`CDChatMessage.contentExpanded` gates the expansion subscription. v41 belongs to chat
RFC-0001 (`kind`, `entryType`) — compaction attributes live in v42 only.

## Deferred (explicit decisions, 2026-07-09 / 2026-07-19)

- Batch splitting above the inline threshold (~2 MB): compaction fails → proxy falls back to
  base queueing.
- Compact-then-swap proxy ordering (upload before route reset) — separate RFC with proxy
  unit tests, includes the per-route failure cooldown.
- Expansion retry window + "permanently unavailable" user surfacing — separate RFC.
