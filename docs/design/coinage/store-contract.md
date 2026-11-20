---
title: "The shared store — contract"
status: "Proposal"
---

# The shared store

Layer 1 ([`coinage-layer-components.md`](coinage-layer-components.md)), layer 2 ([`layer-2-components.md`](layer-2-components.md)) and the durability layer keep their state in **one store**. This document is the contract of that store. The store is the core database, which a native host installs with `set_core_db` before it starts the core's services. Other subsystems that need the same guarantees, including the NFT purses, use this contract instead of defining a second one.

A host app's chat keeps its messages in the app's own store, not in this one. It therefore cannot commit a payment's handoff in the transaction that saves the message, and follows layer 1's ordering rule instead: it commits the handoff before it saves the message (layer 1, G1 invariant 3; [`native-interface.md`](native-interface.md) V2 invariant 2).

## Why one store

Two guarantees in the design need writes by different subsystems to land together or not at all:

- **A payment's handoff and its cheque.** Layer 2 records an outgoing cheque, and layer 1 commits the handoff, in one transaction (layer 2, K1). The transactions that mint the coins were registered when the payment was exported, and they are built only after this commit. If the cheque were recorded without the commit, the coins would be sent while they are still spendable here. If the commit were saved without the cheque, the coins would be marked as handed out although nothing records the cheque that reclaims them.
- **A transaction and what it consumes and creates.** The durability layer registers a transaction, and layer 1 writes its inputs and outputs, in one transaction. Whether a coin is in use or spent is derived from those rows, so the rows must never exist without their transaction, and the transaction must never exist without its rows.

Separate stores could give these guarantees only with a commit protocol between them. One shared store avoids the need for such a protocol.

## Who keeps what

Every fact has exactly one owner. Nothing derived from these facts is stored.

| Fact | Owner |
|---|---|
| Each transaction: its status and its attempts | The durability layer |
| A transaction's inputs and outputs, the operation it belongs to | Layer 1, written in the durability layer's registration transaction |
| What the chain showed for each coin and entry | Layer 1 |
| Which coins' secrets were handed out, committed or not | Layer 1 |
| Derivation counters, the counter for the layer's own operation ids, installations, the privacy policy, purse metadata, a shipped app's hand-over | Layer 1 |
| Receivables, deposits, refunds, outgoing cheques, paid receivables, payment ids | Layer 2 |

## The contract

1. **Serializable transactions that span owners.** One transaction may write rows of any owner. The owner that opens the transaction passes the open transaction to the other owners; the durability engine's two registration callbacks, which plan the records and write layer 1's rows, and layer 1's `commit_export` all take it this way. A failure anywhere rolls back the whole transaction.
2. **A consistent read snapshot.** A reader outside the writer's control flow sees exactly one committed state. The durability layer's completion oracle reads layer 1's rows this way while layer 1 may be writing.
3. **Never restored.** The host excludes the store from every backup and device-transfer mechanism, so after a wipe only the seed remains. Layer 1 cannot tell a restored store from one it has used without interruption, and with a restored store it would re-derive keys it already gave away. The one exception is layer 1's purse metadata, which layer 1 itself exports for backup.
4. **Fails instead of resetting.** A store that cannot be read stops startup. Treating such a store as empty would discard the operations in flight and the marks on coins whose secrets were handed out.
5. **Encrypted at rest, below its owners.** The store holds key handles, the links between a transaction's inputs and outputs, and imported coin secrets until their claims are final, including those of a hand-over's incoming payments. The storage layer underneath the store encrypts it. No owner encrypts its own rows, and no owner sees how the encryption is done.
6. **Rows are kept.** Transactions and their inputs and outputs are never deleted, because whether an asset is spent is read from them.
