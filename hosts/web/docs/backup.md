# Backup (prototype)

The host keeps a backup of a signed-in wallet's local state on the network, with no prompt. A browser that signs in with
the same wallet and holds nothing of it restores that backup before the wallet's chat and products start. It is a
backup, not sync: each browser keeps its own state, and the newest backup wins.

**On by default** for a signed-in wallet, except the public development phrase and the invitation demo's test users. It
runs only where the host's core has the backup accounts; with an older core it logs "Backup: off in this session" and
reads and writes nothing. A backup claims slots and stores on the network for the wallet, with no prompt. Turn it off
per browser, in the DevTools console:

```js
localStorage.setItem("truapi-web-signing-host:backup:switch", "off");
```

The live browser specs turn it off, so they spend on chain only what they test.

Code: `src/backup/` (`host-backup.ts`, `snapshot.ts`, `sealing.ts`, `pointer.ts`), wired in `src/main.ts`. The core's
part: `rust/crates/truapi/src/runtime/signing_host/wallet_account_holder/allowance/backup.rs`.

## What is backed up

One wallet on one network, read from `localStorage`:

- The host's own records of the wallet: the Inbox, the request service's sent and received lists (reply secrets
  included), Nova Chat's peers and pending requests, contacts, product rooms, added product workers and Pocket faces.
- Every product's TrUAPI storage under the wallet, as stored. This holds what products keep there, such as Contact
  Chat's and Kettle Chat's conversation accounts and history, and Kettle's sealed MLS state. The host does not read it.

Keys carry this browser's wallet id. A restore writes them under the other browser's id for the same wallet.

Not backed up: the recovery phrase, all core storage (session, device encryption key, AutoSigning keys, grants and
permission answers), preferences (dock, recents, chain choices) and a product frame's own browser storage.

## How it is kept

1. One tab per browser keeps a wallet's backup: it holds the Web Lock
   `truapi-web-signing-host:backup:<network>:<identity>`. A browser without Web Locks keeps no backup and restores none.
2. Every minute it reads the state. It takes a snapshot when the state changed and then stayed the same for a minute. A
   snapshot is one synchronous read on the page's main thread, where every write of the wallet's host records and
   product storage happens, so it is the state at one moment: what a tab closed at that moment would leave. Writes a
   worker still queues are not in it yet, as they would not be after a crash. It is not a checkpoint that products
   agreed to, so a product is restored as it would restart after a crash. See [Consistency](#consistency).
3. It compresses the snapshot and seals it with XChaCha20-Poly1305. The key is derived from the wallet's entropy for the
   backup alone and bound to the People genesis and the identity account. It is not a chat or account key.
4. The core stores the sealed snapshot on Bulletin under the host's backup Bulletin account. It claims long-term storage
   first when that account's authorization cannot hold it. The host reads the bytes back from the gateway.
5. The core claims a general Statement Store slot for the host's backup statement account. The host waits until the
   allowance holds at the finalized head (`src/platform/allowance-finality.ts`).
6. The host posts a pointer: one statement signed by the backup statement account, on a channel and topic derived from
   the wallet's backup keys. Its sealed data names the snapshot's Bulletin key, its revision, its digest and when it was
   taken. Its expiry lapses 14 days after the snapshot was taken, with the revision in the low 32 bits, and is always
   above the pointer it replaces, so the store keeps the new one. The host checks the signature before it submits, and
   reads the pointer back.
7. Once a UTC day, the browser that published the pointer claims the slot again while the pointer is alive, and posts
   the pointer again, to the same snapshot, if the store lost it. A snapshot that did not change is stored again after
   12 days, before Bulletin lets it lapse.

Both accounts are the core's, hard-derived from the wallet root under the reserved `host-backup` junction, which no
product subtree or product allowance uses. The identity account signs nothing. The backup statement account signs only a
pointer: an expiry at most 15 days ahead, a channel, one topic, and sealed pointer data of at most 1 KiB.

Limits: at most 4 snapshots per wallet per UTC day from one browser, failed tries included; at least 30 minutes between
two; a failure waits 5 minutes, doubling up to 6 hours. A sealed snapshot larger than one Bulletin store
(`MAX_SEALED_BYTES`) is refused, and the log says so. The backup is off while People runs on the light client, where a
backup read would replace the chat's statement topic filter.

## Consistency

What a restore from a snapshot taken at any moment does, by product:

- **Kettle Chat:** Kettle keeps its MLS state, outboxes and acknowledgements in one sealed entry per conversation
  account, writes it before each publish, and recovers a direct chat from an older state with a new group.
  `tests/unit/backup/kettle-restore.test.ts` takes a snapshot after every write of a first browser, restores each in a
  second, and sends and receives with the peer after each, over Kettle's real WebAssembly. From the last snapshot the
  second browser continues the same MLS group.
- **Nova Chat:** the host keeps its peers and pending requests in one entry, and each request's notification account
  comes from the request id and the day. `tests/unit/chat/nova-chat/host.test.ts` restores a copy taken at every write
  of a request and checks that a pending request is posted again under the same account, so no new slot is claimed on
  the same day.
- **Contact Chat:** a request's notification account comes from its post id and the day, so posting it again on the same
  day claims nothing new (`tests/unit/invitations/statement-delivery.test.ts`). A restore at every write is not tested.
- Other products: as their storage survives a crash.

A snapshot is never newer than the moment it was taken. A browser that kept running after it holds a later state; see
[What it does not do](#what-it-does-not-do).

## How it is restored

On sign-in, after the core session is active and before the wallet's chat, workers and products start:

- **This browser holds nothing of the wallet on this network:** under a lock that keeps two tabs from restoring into one
  wallet, the host reads the pointer with the highest expiry it hears, fetches the snapshot from the gateway, checks it
  is the stored bytes, opens it, and checks its schema, network, People genesis and identity and every entry's form.
  Then it writes every entry. A write that fails removes the entries written before it. The session starts with the
  restored state. Nothing is claimed or stored, and the restored browser does not keep the pointer alive: it keeps a
  backup of its own once its state changes.
- **This browser already holds state for the wallet:** it keeps it. When another browser published a different backup,
  the log says so. Nothing is written.
- Any failure is logged, and the session starts as it was. That session keeps no backup, so it cannot replace one it
  failed to read.

The Activity view shows each step under "Backup:".

## What it does not do

- **Two browsers at once.** Each keeps its own backup. The newest pointer wins; nothing merges. A restored browser runs
  the same product state as the one it came from, so using both at once can fork a conversation: Kettle's MLS state and
  Nova Chat's conversation account are written from two places. The first browser cannot be switched off from the
  second.
- **The newest state.** A restore brings back the last snapshot taken, not what the other browser holds now. A fresh
  browser cannot prove that the pointer it found is the newest one.
- **Anything while the publishing browser is closed.** Only the browser that published the pointer claims its slot
  again, while it is open. Once no browser claims it, the pointer lasts until its slot's day plus the runtime's grace
  window ends, and is removed at the next cleanup after that. The snapshot then cannot be found.
- **Quota and expired statements.** Daily slots already used stay used. Statements that expired on the network do not
  come back; only what the browser had stored does.
- **Privacy from timing.** Pointer updates are linkable to the backup statement account, and uploads to the backup
  Bulletin account and their timing. The content is sealed.

## Tests and what they do not cover

- Unit tests (`tests/unit/backup/`): the snapshot, sealing, pointer and service on an in-memory Statement Store and
  Bulletin with a fake core, and Kettle's real MLS state restored (see [Consistency](#consistency)).
- The core's checks: `runtime/signing_host/tests/backup.rs` (the backup account and what it signs), and the Bulletin
  claim stops when the session ends (`runtime/statement_allowance/tests.rs`).
- Browser test (`browser/backup.spec.ts`): two browser profiles of one wallet on the dev server, with the real core and
  a simulated network the test runs (`tools/demos/backup-demo.ts`, dev server only). The simulated network makes the
  backup look every half second and wait a second for quiet.
- Not covered: a live claim, a live Bulletin store, the gateway, a pointer surviving a UTC day boundary on the network,
  a Kettle Chat product running in a restored browser, and a restore at every write of Contact Chat.
