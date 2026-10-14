# Chat Attachments (HOP File Transfer)

File upload/download for chat attachments over HOP (Bulletin chain pool) with an on-chain
bitswap fallback. Implements the base spec's file flow plus chat RFC 0001 (versioned upload
model, inline small files, `bitswap_v1_get` fallback).

## Component Map

Protocol logic lives in `Packages/HandoffService`; the app layer only adapts persistence and
wiring under `polkadot-app/Modules/ChatAttachments/`.

| Component | Where | Role |
|---|---|---|
| `HandoffFileLoader` (`HandoffFileLoading`) | package | Upload/download flows as `AnyAsyncSequence` of progress events |
| `HandoffFileLoadConfig` | package | `chunkSize` (2,000,000) and `inlineMargin` (64); inline threshold = `chunkSize − inlineMargin` |
| `HandoffService` (`HandoffServicing`) | package | `hop_submit` / `hop_claim` / `hop_ack` JSON-RPC |
| `HandoffServiceDecorator` | package | Wraps `HandoffServicing`; claim falls back to `LongTermRemoteStoring` on HOP `NotFound` (1004); skips acks for chain-sourced entries |
| `BitswapRemoteStore` (`LongTermRemoteStoring`) | package | `bitswap_v1_get(cid)` fetch; builds CIDv1 (raw codec, blake2b-256 multihash, base32-lower) via `swift-cid`; verifies `blake2b_256(bytes) == hash` client-side |
| `UploadFileContextProtocol` / `DownloadFileContextProtocol` | package | Persistence seams the app implements |
| `DownloadFileContext`, `UploadFileContext` | app | Actor contexts over `CDMixnetDownload` / `CDMixnetUpload` + attachment files |
| `MixnetDownloadService` / `MixnetUploadService` | app | Subscribe to message streams, drive loaders, publish `AttachmentProgressEvent` |
| `HOPFileLoaderFactory` | app | Wires connection → service → bitswap store → decorator → loader per node |

## Wire Format (RFC 0001)

The root pool entry is a versioned SCALE envelope; indices are normative and pinned by
wire-vector tests (`VersionedUploadedFileTests`):

```
VersionedUploadedFile = v1(UploadedFile) -> 0
UploadedFile          = inline(ByteArray) -> 0 | chunked(ChunkedFile) -> 1
ChunkedFile           = { totalSize: u64, chunks: [ByteArray] }   // legacy layout, byte-for-byte
```

- Files with `size ≤ chunkSize − inlineMargin` upload as a single `inline` entry — one pool
  entry, one claim/ack round-trip. Larger files use the chunked flow with the envelope-wrapped
  chunk list as root entry.
- The recipient learns the shape only by decoding — never from `meta` or message contents.
- Decoding is strict: no legacy (unversioned) fallback. Pre-RFC blobs fail with
  `UploadedFileDecodingError`.

## Invariants

- **Ack follows durable persistence.** The ticket-derived key is the sole recipient, so acking
  removes the pool entry permanently. `saveEntry`/`appendChunk` must complete (atomic file
  writes) before `hop_ack` is sent. Resume paths never re-ack.
- **`NotFound` (1004) on ack is success** (base spec): the entry was already acked elsewhere or
  promoted — `HandoffService.acknowledgeReceivedData` swallows it.
- **Bitswap integrity check is mandatory.** `bitswap_v1_get` has no built-in verification;
  `BitswapRemoteStore` hashes the returned bytes and treats a mismatch as not-found (logged).
- **Broken persisted download state is kept, not cleaned.** If a stored metadata blob fails
  decoding on resume, the loader throws `invalidResumeMetadata` and the `CDMixnetDownload` row
  stays — it is evidence for parsing bugs. A dedicated cleanup service may collect such rows
  later. Do not add self-healing deletion to this path.

## Persistence

`CDMixnetDownload.entryType` (v41): `0` = metadata/chunked root, `1` = inline. For inline
entries the context persists the file bytes and the DB row in one `saveEntry` call — there is
no separate chunk append trip. Chunked resume uses `lastChunkIndex` with a DB-ahead-of-file
rollback guard.

## Deferred (RFC 0001 scope cuts)

- Persisted retry policy for the bitswap fallback (24h window, exponential backoff, node
  rotation, "permanently unavailable" terminal state). The decorator does a single fallback
  attempt per claim; retries happen at the download-restart level.
