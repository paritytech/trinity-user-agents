package io.paritytech.polkadotapp.feature_coinage_impl.data.transaction

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CheckpointBlock
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageInput
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageTransactionId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus

/** An asset's on-chain identity: a coin's derived account id, or a voucher's ring VRF public key. */
typealias AssetPublicKey = DataByteArray

/**
 * One transaction as the rules see it: the engine's row joined to the assets coinage holds for it.
 *
 * The engine half is domain-neutral and lives in the shared ledger; the asset half is coinage's alone.
 */
data class LedgerEntry(
    val entry: DurableTxEntry,
    val inputs: List<LedgerAsset>,
    val outputs: List<LedgerAsset>,
) {
    val id: CoinageTransactionId get() = entry.id

    val groupId: CoinageOperationGroupId? get() = entry.groupId

    val txHash: String get() = entry.txHash

    val checkpoint: CheckpointBlock get() = entry.checkpoint

    val mortalityBlocks: Long get() = entry.mortalityBlocks

    val status: DurableTxStatus get() = entry.status

    val successDetectedAt: CheckpointBlock? get() = entry.successDetectedAt

    /** The last block this transaction can still execute in. */
    val mortalityEnd: Long get() = entry.mortalityEnd
}

enum class CoinageAssetKind { COIN, VOUCHER }

/** [asset] is null for a coin whose key a peer sent us: it has an on-chain identity but no local one. */
data class LedgerAsset(
    val kind: CoinageAssetKind,
    val asset: OwnAsset?,
    val publicKey: AssetPublicKey,
) {
    val isCoin: Boolean get() = kind == CoinageAssetKind.COIN
    val isVoucher: Boolean get() = kind == CoinageAssetKind.VOUCHER

    /**
     * Absence at a head is proof this asset was consumed, because its identity is never reused.
     *
     * A coin's account qualifies. A voucher's member key does not: ring cleaning removes it while the
     * voucher is still redeemable, so its disappearance proves nothing either way.
     */
    val absenceProvesConsumption: Boolean get() = kind == CoinageAssetKind.COIN

    /**
     * The chain carries a positive consumption proof for this asset, beyond mere absence.
     *
     * A voucher's recycler alias qualifies. A coin has no such signal — `CoinsByOwner` being empty is
     * necessary but not sufficient for a spend — so absence is the only evidence it can offer.
     */
    val hasConsumptionProof: Boolean get() = kind == CoinageAssetKind.VOUCHER
}

fun LedgerAsset.asCoinOrNull(): OwnAsset.Coin? = asset as? OwnAsset.Coin

fun LedgerAsset.asVoucherOrNull(): OwnAsset.Voucher? = asset as? OwnAsset.Voucher

data class RegistrationInput(
    val input: CoinageInput,
    val publicKey: AssetPublicKey,
)

data class RegistrationOutput(
    val output: OwnAsset,
    val publicKey: AssetPublicKey,
)
