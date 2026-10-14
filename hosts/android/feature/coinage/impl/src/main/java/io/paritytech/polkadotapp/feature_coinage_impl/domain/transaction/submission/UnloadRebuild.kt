package io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.VoucherRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.EntryAssets
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.asCoinOrNull
import io.paritytech.polkadotapp.feature_coinage_impl.data.transaction.asVoucherOrNull
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.builders.UnloadExtrinsicBuilder
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.ActivePeopleCollectionUseCase
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.ScheduledDurableTx
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

/**
 * A payment's recycler unloads: the vouchers each one redeems and the coins it mints, in order.
 *
 * Every unload of one call is built together, so they share one pinned block and person proof and each gets its
 * own free unload token. A voucher counts as present while it sits in a recycler: that is where an unload proves
 * it, and a voucher that left one was redeemed by something else.
 */
class UnloadRebuild @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val coinRepository: CoinRepository,
    private val voucherRepository: VoucherRepository,
    private val activePeopleCollectionUseCase: ActivePeopleCollectionUseCase,
    private val unloadExtrinsicBuilder: UnloadExtrinsicBuilder,
) : CoinageRebuild<UnloadRebuild.Unload, CoinageKeyIndex> {
    class Unload(val voucherIndices: List<CoinageKeyIndex>, val outputs: List<Coin>)

    override fun termsOf(params: DataByteArray): RebuildTerms? =
        CoinageSubmissionParams.decodeTransfer(params).getOrNull()?.let { RebuildTerms(it.buildUntil, it.retryFailures) }

    override suspend fun resolve(
        transactions: List<ScheduledDurableTx>,
        assets: Map<DurableTxId, EntryAssets>,
    ): Map<DurableTxId, Unload> {
        val coins = outputCoinsOf(assets.values)
        val knownVouchers = knownVouchersOf(assets.values)

        return transactions.mapNotNull { tx ->
            val entry = assets[tx.id] ?: return@mapNotNull null
            val inputs = validInputs(entry, knownVouchers) ?: return@mapNotNull null
            val outputs = validOutputs(entry, coins) ?: return@mapNotNull null

            tx.id to Unload(inputs, outputs)
        }.toMap()
    }

    private suspend fun outputCoinsOf(entries: Collection<EntryAssets>): Map<CoinageKeyIndex, Coin> {
        val indices = entries.flatMap { entry -> entry.outputs.mapNotNull { it.asCoinOrNull()?.derivationIndex } }

        return coinRepository.getCoinsBy(indices).associateBy { it.derivationIndex }
    }

    private suspend fun knownVouchersOf(entries: Collection<EntryAssets>): Set<CoinageKeyIndex> {
        val indices = entries.flatMap { entry -> entry.inputs.mapNotNull { it.asVoucherOrNull()?.ringVrfIndex } }

        return voucherRepository.getByRingVrfKeyIndices(indices).mapToSet { it.ringVrfKeyIndex }
    }

    /** Every input a voucher we still hold, or nothing: an unload redeems exactly what was recorded. */
    private fun validInputs(entry: EntryAssets, knownVouchers: Set<CoinageKeyIndex>): List<CoinageKeyIndex>? {
        if (entry.inputs.isEmpty()) return null

        return entry.inputs.map { input ->
            input.asVoucherOrNull()?.ringVrfIndex?.takeIf { it in knownVouchers } ?: return null
        }
    }

    /** Every output a coin we recorded, or nothing: a partial unload would mint something else. */
    private fun validOutputs(entry: EntryAssets, coins: Map<CoinageKeyIndex, Coin>): List<Coin>? =
        entry.outputs.map { output -> output.asCoinOrNull()?.let { coins[it.derivationIndex] } ?: return null }

    override fun inputsOf(transaction: Unload): Set<CoinageKeyIndex> = transaction.voucherIndices.toSet()

    override suspend fun presence(inputs: Set<CoinageKeyIndex>): Flow<Set<CoinageKeyIndex>> =
        voucherRepository.subscribeVouchersInRecycler().map { vouchers -> vouchers.mapToSet { it.ringVrfKeyIndex } }

    override suspend fun build(transactions: List<Unload>): Result<List<EnrichedSendableExtrinsic>> {
        // Re-read after the look, so every voucher carries the recycler location it is proven in now.
        val vouchers = voucherRepository.getByRingVrfKeyIndices(transactions.flatMap { it.voucherIndices })
            .associateBy { it.ringVrfKeyIndex }

        val unloads = transactions.map { unload ->
            UnloadExtrinsicBuilder.Unload(
                vouchers = unload.voucherIndices.map { vouchers.getValue(it) },
                outputCoins = unload.outputs,
            )
        }

        return with(StalenessReportCollector.NoOp) {
            unloadExtrinsicBuilder.build(
                unloads = unloads,
                peopleCollection = activePeopleCollectionUseCase.getActivePeopleCollection(),
                chain = chainAssetProvider.chain(),
            )
        }.onSuccess { unloadExtrinsicBuilder.noteUnloadsHappened(it.size) }
    }
}
