# Deviations from RFC-6 and RFC-17

This document lists where the design and the RFCs it serves disagree. The design is [`coinage-layer-components.md`](coinage-layer-components.md) (layer 1) and [`layer-2-components.md`](layer-2-components.md) (layer 2). In each case the design is kept, and the RFC is to be amended to match it.

- [RFC-6: Payment Host API](https://github.com/paritytech/host-rust-core/blob/main/docs/rfcs/0006-payments.md)
- [RFC-17: Coinage Payment User Agent API](https://github.com/paritytech/host-rust-core/blob/main/docs/rfcs/0017-coinage-payment.md)
- [RFC-0021: Add Coins variant to PaymentTopUpSource](https://github.com/paritytech/host-rust-core/blob/main/docs/rfcs/0021-payment-topup-coins.md)

Code follows the notation defined in layer 1's section 2.

## Deviations

### 1 · Derivation paths — RFC-17 Appendix A

**RFC-17:** one shape for every Coinage key, with a page index and a derivation secret concatenated into one junction:

```text
//coinage//<PURSE>//<PAGE><DERIV_SEC>/<ITEM>
```

**This design:** the scheme the shipped wallet uses. It cannot change without leaving existing funds unreachable:

```text
coins     //coinage//<purse>//<installation>/<item>            sr25519
entries   //coinage-ring-vrf//<purse>//<installation>//<item>  bandersnatch
```

- `<installation>` is 32 random bytes, written as a `0x`-prefixed hex junction, so that its chain code is exactly those 32 bytes. It replaces `<PAGE><DERIV_SEC>`. Its purpose is that a reinstall never re-derives an item that the previous installation already handed out.
- Recycler entries have their own root. RFC-17 does not mention them.
- The main purse is `u32::MAX`, as in RFC-17.

**Amend:** replace Appendix A with the paths above.

### 2 · Purse identifiers are linear and may recur — RFC-17 *Purse*, `create_purse`

**RFC-17:** "Non-main identifiers are randomly assigned by the user agent."

**This design:** product purses count up from zero, and an identifier can recur across reinstalls.

- **Linear**, because counting up is the only assignment that a reconstruction can enumerate from the seed. Random identifiers that are stored nowhere cannot be found again.
- **May recur**, because the product keeps the identifier and presents it again after a reinstall. The keys do not recur, because every derivation path has the installation below the purse.
- **Cost:** a product can infer how many purses were created before its own.

Decided with Gav: identifiers are flat, so every asset can be recovered from the recovery phrase alone. The phrase cannot tell which product a purse belongs to. That ownership is restored on a best-effort basis from a backup of the purse metadata. If the restore fails, the user still has the assets.

The metadata backup is a recovery system separate from the phrase, eventually Polkadot Cloud. Release R2 needs no such backup, because it has no third-party products. Release R3 gets a basic cloud backup.

A purse holds only the coins and recycler entries under the current installation. Everything found under an earlier installation counts toward the main purse, whatever purse identifier it was found under. This also applies to a purse restored from the backup: it comes back with its owner, but empty. So when an identifier recurs after a reinstall, the product holding it never gets value from before the reinstall.

**Amend:** drop "randomly assigned"; state that identifiers are assigned in order and remain meaningful across reinstalls.

### 3 · A cheque's progress is read by a separate call — RFC-17 `create_cheque`

**RFC-17:** returns `Result<CoinPaymentCheque, CoinPaymentError>` directly.

**This design:** layer 1 hands out a payment's coin secrets as soon as the payment is reserved, and mints the coins afterwards, possibly after waiting for value that is still settling. So the cheque exists at once, as in Polkadot Mobile, but at that moment the payer does not know how much of it the payee can claim. RFC-17 gives the paying product no way to learn this later, or to learn whether the cheque was claimed.

A payment is never refused because its value is still settling, and no completion time is predicted. Whatever the payee cannot claim by the claim deadline (gap 5), the payer's user agent reclaims.

**Amend:** keep `create_cheque` as shipped, returning the cheque at once, and add a call for its progress:

```rust
fn cheque_status(cheque: CoinPaymentCheque)          // also after a reload
    -> Result<Subscription<CoinPaymentChequeProgress>, CoinPaymentError>;

struct CoinPaymentChequeProgress {
    minting: Balance,      // not yet claimable
    claimable: Balance,    // minted, not claimed
    claimed: Balance,      // claimed by the payee, final
    returned: Balance,     // reclaimed, cancelled, or never minted
}
```

The stream closes when nothing is minting or claimable.

### 4 · Recovery from secret material covers value, not operations — RFC-17 behavioural requirement 16

**RFC-17:** "Purses, receivables, pending cheques, deposit/refund operations, and clearing references must be durable across user-agent restart. Hosts must have a recovery path from user-agent-controlled secret material."

**This design:** meets the first sentence: everything it lists survives a restart. The second sentence is about losing local state entirely. From the seed alone, the design recovers the funds (coins and recycler entries) into the main purse, and nothing else. It does not recover the purses themselves, which come back only from a metadata backup (gap 6), nor which product a purse belongs to, nor any receivable, operation or clearing reference. These exist only in local state, and nothing on chain allows deriving them again. Keeping them across a reset of the host would require storing the local state outside the app, which this design does not cover.

**Amend:** scope the second sentence to funds, recovered into the main purse.

### 10 · Channel topics are derived from receivables — RFC-17 `CoinPaymentTransmissionChannel`

**RFC-17:** the standard channel carries an explicit `sss_topic`. Its unresolved questions include deriving it from the receivable instead.

**This design:** every topic is derived from a receivable by a hash, with a different label for each purpose. The payment topic comes from the payee's receivable. The refund topic comes from the payer's return receivable, which travels encrypted inside the cheque. So someone who saw the invoice (often a QR code shown in public) cannot watch for its refund. No invoice or cheque carries a topic.

```text
payment_topic = BLAKE2b-256("coinage/payment-topic" ‖ receivable)
refund_topic  = BLAKE2b-256("coinage/refund-topic"  ‖ return_receivable)
```

**Amend:** define both derivations. Keep the standard channel's `sss_topic`, as shipped, and define it as the payment topic derived from the receivable. Define what a statement under a topic holds: `SCALE(ChannelItem)`, which is the cheque itself whenever it fits in a statement, and otherwise HOP's reference to the cheque (layer 2, M1).

### 11 · Invoices have one standard encoding — RFC-17 *Invoice*

**RFC-17:** "The invoice URI scheme and payload encoding are product-layer concerns."

**This design:** a payer's user agent pays an invoice without any product (RFC-17's own premise), so it must be able to read every product's invoices. The encoding is therefore fixed: the SCALE encoding of `Invoice { version, to, amount }`, with version 1, written as `coinage-invoice:` followed by the unpadded base64url of those bytes. This text is what a QR code holds, and what a deep link carries in its `invoice` query parameter. The invoice has no channel field, because the topic is derived from the receivable (entry 10).

**Amend:** define the encoding in RFC-17 instead of leaving it to products.

### 12 · Top-up secrets are 64-byte sr25519 secrets — RFC-0021 `PaymentTopUpSource`

**RFC-0021:** `Coins { sr25519_secret_keys: Vec<[u8; 32]> }` and `PrivateKey { ed25519_private_key: [u8; 32] }`.

**This design:** both carry 64-byte sr25519 secrets, as the shipped wire and RFC-6 do. A 32-byte mini secret cannot represent a coin key of this design: every coin key is derived through a soft junction (layer 1, A2), and a soft junction produces only the 64-byte expanded secret, not a mini secret.

**Amend:** RFC-0021's variants take 64-byte sr25519 secrets, as RFC-6 and the wire do. Every such secret, including the coin secrets in a cheque, is in the canonical layout: first the 32-byte little-endian encoding of the secret scalar, then the 32-byte nonce (layer 1, A2). Both shipped apps send this layout. A secret in any other layout, such as the ed25519-compatible layout, which stores eight times the scalar, is refused, not reinterpreted. The reason is that about one in eight byte strings in the ed25519-compatible layout is also a valid canonical secret, for a different key (layer 1, G1 invariant 5).

### 13 · A deposit is done only at the expected amount, and otherwise refunded — RFC-17 `deposit`, `CoinPaymentStatus`

**RFC-17:** `Done` means "All coins cleared". A `Failed` deposit may have cleared part of the value, and "product-specific refund policy decides how to handle failure".

**This design:** a cheque declares its amount, but only the chain shows what its coins are actually worth. A deposit is `Done` only when the value that cleared equals the cheque's amount. Any other outcome (a coin missing, sniped, or worth a different amount than declared) is `Failed`. In that case the payee's user agent refunds the value that cleared by itself, without the product. So the payee never keeps a payment it did not agree to, and the payer gets back what arrived without anyone having to decide.

**Amend:** define `Done` as "cleared the cheque's amount", and require a failed deposit to be refunded automatically when its cheque carries a return path.

### 14 · A status may carry an empty clearing reference — RFC-17 `CoinPaymentStatus`

**RFC-17:** `Done` and `Failed` carry a `CoinPaymentClearingReference`.

**This design:** a rebalance or a deletion moves value between the user's own purses, and a cancellation takes value back to the payer. None of these is a payment that a reference could prove, so H4 builds no reference for them.

**Amend:** define the **empty reference** (an all-zero root with no leaves) as naming no transactions, and send it in these three cases.

### 15 · A low balance is reported only to a caller that may see the balance — RFC-17 `BalanceLow`

**RFC-17:** a failure distinguishes a low balance from a denial.

**This design:** telling a product that its purse is short reveals information about the balance. So `BalanceLow` goes only to a caller that may query the purse. Any other caller gets `Denied`, as RFC-6 already does for `InsufficientBalance` (layer 2, H5 invariant 2).

**Amend:** report `BalanceLow` only to callers with query access, as RFC-6 does.

### 17 · The `CoinPayment` permission is asked for on first use — RFC-17 *Permissions*

**RFC-17:** adds `UserAgentPermission::CoinPayment`, which a product requests.

**This design:** the shipped wire has no `CoinPayment` variant. The user agent asks for the permission itself, on the first call that needs it (layer 2, J1 invariant 5). RFC-6 calls on the main purse use RFC-6's own consent and do not need the `CoinPayment` permission, so products built on the shipped RFC-6 calls keep working.

**Amend:** add the `CoinPayment` request to the wire, or state that the user agent asks for it on first use; exempt RFC-6 calls on the main purse.

## Gaps

These are not disagreements with RFC-17. They are RFC-17 behaviour that cannot be implemented as written.

### 5 · A refund needs a return path — RFC-17 `refund`, `CoinPaymentCheque`

`refund(receivable)` returns everything that arrived under a receivable to the payer, without the payer taking part. Nothing in a cheque says where the payer can be reached: it carries only the receivable, the amount and the encrypted secrets.

**Decided:** the cheque carries a **return receivable**. This is a fresh receivable of the payer's, bound to the purse the payment came from. It is inside the encrypted part, so only the payee learns it. A refund is an ordinary cheque sealed to the return receivable. It lands in fresh accounts like any payment, and it is posted to the refund topic derived from that return receivable (deviation 10). A refund cheque carries no return receivable of its own, so a refund cannot itself be refunded. Paying back into the payer's original coin accounts was rejected: it would reuse accounts, which links the refund to the payment on chain.

**How long the payer watches: two parameters, set by the implementation.**
- A **claim timeout**: how long after a payment the payee may claim it. After that time, the payer may reclaim the coins without competing with a legitimate claim. Nothing else limits when a claim can happen: neither the chain nor RFC-17 sets a limit, and an unclaimed coin does not age. Without a claim timeout, the payer could never stop watching.
- A **refund window**: how long after the claim timeout the payer keeps watching the refund topic.

The cheque carries its claim deadline, set by the payer, so payer and payee use the same instant even when their configured timeouts differ. Both keep network time, and the claim timeout is many hours long, so the difference between their clocks is small compared to it. The payer reclaims only after the deadline, the maximum claim validity window and a clock tolerance have all passed. So the payer's reclaim never competes with a claim made before the deadline.

When two implementations set different refund windows, nothing is lost. A refund sent after the payer stopped watching stays unclaimed. The payee's user agent then reclaims it at its deadline, and its value can be refunded again.

**Amend:** carry the return receivable and the claim deadline inside `encrypted_secrets`, preceded by a version byte, so the cheque keeps RFC-17's three fields (layer 2, H3). Fix protocol-wide maximums for the claim timeout, the clock tolerance and the validity window of a claim transaction. Then a payee never refuses a cheque whose deadline the payer chose within these maximums, and a payer's reclaim never competes with the claim of a payee running another implementation. Leave the actual values, up to these maximums, and the refund window as parameters that each implementation sets. Add an error for a refund that has no return path. Since every payment cheque carries a return receivable, this error occurs only for a cheque that lacks one.

### 6 · A purse's creator does not survive a reinstall — RFC-17 *Purse*

RFC-17 grants the product that created a purse default access to it. Which product created a purse is recorded only in local state. After a reinstall, only the purse-metadata backup (deviation 2) can provide it.

- **With a backup**, the purse comes back with its creator, and empty: its earlier coins count toward the main purse.
- **Without a backup**, the earlier identifier names no purse of that product's. Its coins are in the main purse, and the product creates a new purse.

So no purse ever exists without a known creator, and there is never a purse without an owner to grant access to. Release R2 has no backup, but it has no third-party products either.

**Amend:** state that after a reinstall a product's purse returns only from a backup, empty, and that its earlier value is in the main purse.

### 7 · A payer cannot cancel a cheque — RFC-17 has no payer-side counterpart to `refund`

Once `create_cheque` has run, the coins in the cheque no longer count in the payer's balance. If the cheque is never deposited (because it was lost in transport, or the invoice was abandoned), nothing in RFC-17 brings the coins back. Layer 1 can bring them back: it can still derive the secrets of coins it exported, so it can move any coin that is still unclaimed into a fresh account. If the payee claimed a coin first, the payee keeps it.

**Amend:** add a payer-side call, `cancel_cheque(cheque)`, returning the same status stream as `refund`. Decided: the call is explicit, and it serves to cancel before the claim deadline. After the claim deadline (gap 5), the payer's user agent reclaims whatever is still unclaimed by itself. This also recovers a cheque that a crash left behind before it was ever handed over. So `create_cheque` needs no identifier to be safe to retry.

### 8 · A product cannot find its purses — RFC-17 `create_purse`, `query_purse`

A product can create a purse or query one whose identifier it already holds. It cannot ask which purses are its own. After a recovery, or once its own storage is gone, it cannot find them again. One product also cannot discover a purse that another product wants to share with it.

**Amend:** add a call that lists the purses a product may access: its own, and any purse the user has granted it query access to. A product never learns of another product's purses unless the user granted it access, because listing them would tell it which other products the user has. The call returns each purse with its identifier, because `CoinPaymentPurseInfo` does not contain one.

### 9 · Verifiable clearing references reveal the payer's coins — RFC-17 `CoinPaymentClearingReference`

Products need clearing references that a third party can verify, which means that the references point at the chain transactions that claimed the payment. RFC-17 also requires that they expose no "source coin IDs". Both requirements cannot hold together. A claim transaction is signed by the coin it claims, so its hash leads directly to the payer's coin, and from the coin to the transaction that minted it. If that transaction was a split, it also minted the payer's change.

The receiving user agent already knows these coins, since it held their secrets. What is new is that the receiving product learns them too, and so does anyone the product shows the reference to.

Two further consequences:
- **A refund's reference names the payer's fresh account** directly, because the transaction the reference points at is the payer's claim of the refund. The product that sent the refund can follow what the payer does with that money next.
- **The receiving product learns its own purse's coin accounts**, which it has no use for, because every transaction names its destination accounts. Moving value out of a purse is a transfer on chain, so the product can follow the value, for example into the main purse. Whether such moves go through the recycler first is decided by the user's privacy policy (layer 1, A4).

A payment's clearing references go only to the receiving product; `create_cheque` gives the payer none. Gav's sample API already exposes all of this, because its leaves are transaction hashes. It is RFC-17's privacy requirement that conflicts with this structure.

**Decided:** keep Gav's structure. The reference names the transactions, and its root is a Merkle root over the leaves, so a product can prove one claim to a third party without revealing the payer's other coins. Layer 2 fixes the construction (H4).

**Amend:** state that a clearing reference identifies the payment's transactions, including the coins the payer spent, and define the Merkle construction. Drop "without exposing … source coin IDs" for clearing references, and limit the matching clauses (in the summary, behavioural requirement 4 and the privacy requirements) in the same way.

