package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.recovery

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.ensureKeysWithNullDefault
import io.paritytech.polkadotapp.common.utils.filterNotNull
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CheckpointBlock
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.OwnAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.derivation.VoucherRingDerivation
import io.paritytech.polkadotapp.feature_coinage_impl.data.model.OnChainAliasState
import io.paritytech.polkadotapp.feature_coinage_impl.data.signer.context.CoinageSigningContextProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.AssetPublicKey
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.CoinageStateReader
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerAsset
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.LedgerEntry
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.RecyclerAliasKey
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogW
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.logId
import io.paritytech.polkadotapp.feature_members_api.data.model.RingPosition
import io.paritytech.polkadotapp.feature_members_api.data.model.ringIndex
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.PinnedChainView
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import javax.inject.Inject

/**
 * Turns one entry plus a pinned view into the [ChainEvidence] the rules read.
 *
 * Every read is independent: one failing marks only its own keys unknown, and costs only the rules that
 * needed them rather than aborting the others.
 */
class CoinageEvidenceCollector @Inject constructor(
    private val voucherRingDerivation: VoucherRingDerivation,
    private val coinageSigningContextProvider: CoinageSigningContextProvider,
) {
    suspend fun collect(
        entry: LedgerEntry,
        reader: CoinageStateReader,
        view: PinnedChainView,
    ): ChainEvidence = coroutineScope {
        val assets = entry.inputs + entry.outputs
        val coinKeys = assets.filter { it.isCoin }.map { it.publicKey }.distinct()
        val voucherAssets = assets.filter { it.isVoucher }.distinctBy { it.publicKey }
        val logId = entry.logId()

        // Independent reads, so they go out together rather than one head after the other.
        val coinsAtFinalized = async { reader.coinPresence(logId, view.finalizedHead, coinKeys) }
        val coinsAtBest = async { reader.coinPresence(logId, view.bestHead, coinKeys) }
        val vouchersAtFinalized = async { reader.voucherEvidence(logId, view.finalizedHead, voucherAssets) }
        val vouchersAtBest = async { reader.voucherEvidence(logId, view.bestHead, voucherAssets) }

        val finalized = vouchersAtFinalized.await()
        val best = vouchersAtBest.await()

        ChainEvidence(
            finalized = view.finalizedHead,
            best = view.bestHead,
            presenceAtFinalized = coinsAtFinalized.await() + finalized.presence,
            presenceAtBest = coinsAtBest.await() + best.presence,
            aliasAtFinalized = finalized.alias,
            aliasAtBest = best.alias,
        )
    }

    /**
     * A voucher against the chain at one block, resolved in two steps: the denomination whose recycler it is
     * a member of, then where it sits in that denomination's collection.
     *
     * Nothing here comes from the local voucher row. Its ring index is written from a best-block
     * subscription and can name a ring the voucher has already left, which would send the alias read to a
     * key that answers "no entry" — indistinguishable from a voucher that was never unloaded.
     */
    private suspend fun CoinageStateReader.voucherEvidence(
        logId: String,
        at: CheckpointBlock,
        vouchers: List<LedgerAsset>,
    ): VoucherEvidence {
        if (vouchers.isEmpty()) return VoucherEvidence(emptyMap(), emptyMap())

        val memberKeys = vouchers.map { it.publicKey }.distinct()
        val memberships = recyclerMembershipsAt(at.blockHash, memberKeys).getOrElse {
            coinageLogW("$logId read-unknown what=recycler-membership at=${at.blockNumber} vouchers=${memberKeys.size} error=$it")

            return VoucherEvidence.unknown(memberKeys)
        }
        val positions = ringPositionsOf(at, memberships).getOrElse {
            coinageLogW("$logId read-unknown what=ring-position at=${at.blockNumber} vouchers=${memberKeys.size} error=$it")

            return VoucherEvidence.unknown(memberKeys)
        }

        val presence = positions.mapValues { (_, position) ->
            // A voucher has no other existence signal: archival removes the membership while the voucher is
            // still redeemable, so anything short of a resolved position is unknown rather than absent.
            if (position != null) ChainPresence.PRESENT else ChainPresence.UNKNOWN
        }

        val aliasKeys = aliasKeysFor(vouchers, memberships, positions)

        // Not unwrapped: a voucher with no alias key needs no alias read, so a failed one must not reach it.
        val aliasStates = aliasStatesAt(at.blockHash, aliasKeys.values.filterNotNull())
            .onFailure { coinageLogW("$logId read-unknown what=alias-state at=${at.blockNumber} error=$it") }

        val alias = memberKeys.associateWith { key ->
            aliasRead(positions.getValue(key), aliasKeys.getValue(key), aliasStates)
        }

        return VoucherEvidence(presence = presence, alias = alias)
    }

    private suspend fun CoinageStateReader.ringPositionsOf(
        at: CheckpointBlock,
        memberships: Map<AssetPublicKey, ValueExponent?>,
    ): Result<Map<AssetPublicKey, RingPosition?>> {
        val known = memberships.filterNotNull()

        return ringPositionsAt(at.blockHash, known)
            .map { it.ensureKeysWithNullDefault(memberships.keys) }
    }

    private fun aliasRead(
        position: RingPosition?,
        aliasKey: RecyclerAliasKey?,
        aliasStates: Result<Map<RecyclerAliasKey, OnChainAliasState?>>,
    ): AliasRead = when {
        position == null -> AliasRead.UNKNOWN

        // Onboarding means never yet placed in a ring, and an unload writes an alias under a ring index, so
        // no unload can have happened. Established without reading anything, which is why it is answered
        // before the read is consulted. Suspended carries no ring index and may have been unloaded before it
        // was suspended, so that one stays unknown.
        position is RingPosition.Onboarding -> AliasRead.NOT_UNLOADED

        aliasKey == null || aliasStates.isFailure -> AliasRead.UNKNOWN

        aliasStates.getOrThrow().getValue(aliasKey) is OnChainAliasState.Unloaded -> AliasRead.UNLOADED

        else -> AliasRead.NOT_UNLOADED
    }

    /**
     * Every voucher present, with null where no alias key can be formed — either read failed, the voucher is
     * in no recycler, or it is in no ring to key an alias under. A voucher missing from this map would be
     * indistinguishable from one that simply has no key, which is the ambiguity the reads themselves avoid.
     */
    private suspend fun aliasKeysFor(
        vouchers: List<LedgerAsset>,
        memberships: Map<AssetPublicKey, ValueExponent?>,
        positions: Map<AssetPublicKey, RingPosition?>,
    ): Map<AssetPublicKey, RecyclerAliasKey?> {
        val aliasContext = coinageSigningContextProvider.recyclerVouchersContext()

        return vouchers.associate { voucher ->
            voucher.publicKey to aliasKeyOf(voucher, memberships, positions, aliasContext)
        }
    }

    private suspend fun aliasKeyOf(
        voucher: LedgerAsset,
        memberships: Map<AssetPublicKey, ValueExponent?>,
        positions: Map<AssetPublicKey, RingPosition?>,
        aliasContext: BandersnatchContext,
    ): RecyclerAliasKey? {
        val index = voucher.voucherIndex() ?: return null
        val denomination = memberships.getValue(voucher.publicKey) ?: return null
        val ringIndex = positions.getValue(voucher.publicKey)?.ringIndex ?: return null
        val alias = voucherRingDerivation.aliasOf(index, aliasContext)

        return RecyclerAliasKey(
            valueExponent = denomination.value.toBigInteger(),
            recyclerIndex = ringIndex.value,
            alias = alias.value.toDataByteArray(),
        )
    }
}

private class VoucherEvidence(
    val presence: Map<AssetPublicKey, ChainPresence>,
    val alias: Map<AssetPublicKey, AliasRead>,
) {
    companion object {
        fun unknown(keys: Collection<AssetPublicKey>): VoucherEvidence {
            return VoucherEvidence(
                presence = keys.associateWith { ChainPresence.UNKNOWN },
                alias = unknownAliases(keys)
            )
        }

        fun unknownAliases(keys: Collection<AssetPublicKey>): Map<AssetPublicKey, AliasRead> {
            return keys.associateWith { AliasRead.UNKNOWN }
        }
    }
}

private fun LedgerAsset.voucherIndex() = (asset as? OwnAsset.Voucher)?.ringVrfIndex

/**
 * Presence of the entry's coins at one block.
 *
 * A failed read is the whole batch failing; there is no per-key channel below this. Within a successful read
 * every requested key is present, so a null value is the chain holding no coin there — absent.
 */
private suspend fun CoinageStateReader.coinPresence(
    logId: String,
    at: CheckpointBlock,
    coinKeys: List<AssetPublicKey>,
): Map<AssetPublicKey, ChainPresence> {
    val coinStates = coinsAt(at.blockHash, coinKeys)
        .onFailure { coinageLogW("$logId read-unknown what=coin-presence at=${at.blockNumber} coins=${coinKeys.size} error=$it") }
        .getOrNull()

    return coinKeys.associateWith { key ->
        when {
            coinStates == null -> ChainPresence.UNKNOWN
            coinStates.getValue(key) != null -> ChainPresence.PRESENT
            else -> ChainPresence.ABSENT
        }
    }
}
