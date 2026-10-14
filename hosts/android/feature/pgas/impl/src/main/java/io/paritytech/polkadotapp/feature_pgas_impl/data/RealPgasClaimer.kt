@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_pgas_impl.data

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchContext
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.common.utils.mapErrorNotInstance
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.common.utils.progressStallReport.markRegion
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_people_api.domain.BandersnatchKeyResolver
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.ActivePeopleCollectionUseCase
import io.paritytech.polkadotapp.feature_pgas_api.data.calls.claimPgas
import io.paritytech.polkadotapp.feature_pgas_api.data.calls.pgas
import io.paritytech.polkadotapp.feature_pgas_api.domain.OnExistingAllocationStrategy
import io.paritytech.polkadotapp.feature_pgas_api.domain.PgasChainAssetProvider
import io.paritytech.polkadotapp.feature_pgas_api.domain.PgasClaimError
import io.paritytech.polkadotapp.feature_pgas_api.domain.PgasClaimer
import io.paritytech.polkadotapp.feature_pgas_impl.data.extension.pgasClaim
import io.paritytech.polkadotapp.feature_pgas_impl.data.repository.PgasRepository
import io.paritytech.polkadotapp.feature_pgas_impl.data.signer.origins.PgasOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.flattenExecutionFailure
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ResubmitWhenValidFactory
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Clock
import kotlin.time.Duration.Companion.days
import kotlin.time.ExperimentalTime
import io.paritytech.polkadotapp.common.R as RCommon

private val SECONDS_PER_PERIOD: Long = 1.days.inWholeSeconds

class RealPgasClaimer @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val extrinsicService: ExtrinsicService,
    private val resubmitWhenValidFactory: ResubmitWhenValidFactory,
    private val pgasOrigins: PgasOrigins,
    private val pgasRepository: PgasRepository,
    private val bandersnatchKeyResolver: BandersnatchKeyResolver,
    private val activePeopleCollectionUseCase: ActivePeopleCollectionUseCase,
    private val pgasChainAssetProvider: PgasChainAssetProvider,
    private val tokenBalanceTypeRegistry: TokenBalanceTypeRegistry,
    private val dotNsTldProvider: DotNsTldProvider,
) : PgasClaimer {
    context(diagnostics: StalenessReportCollector)
    override suspend fun claim(destinationAccountId: AccountId, strategy: OnExistingAllocationStrategy): Result<Unit> = diagnostics.markRegion(RCommon.string.pgas_stall_claiming) {
        Timber.i("starting claim for destination=$destinationAccountId, strategy=$strategy")

        resolveClaimContext(destinationAccountId).flatMap { ctx ->
            if (strategy == OnExistingAllocationStrategy.IGNORE && ctx.previousBalance.isPositive()) {
                Timber.i("existing balance=${ctx.previousBalance}; strategy=IGNORE — skipping claim")
                return@flatMap Result.success(Unit)
            }
            pickFreeSlotIndex(ctx.chain.id, ctx.period, ctx.collection)
                .mapError { PgasClaimError.NoAllocationAvailable(it) }
                .flatMap { slotIndex -> submitClaim(ctx, slotIndex, destinationAccountId) }
        }
            .onFailure { Timber.e(it, "claim failed") }
            .mapErrorNotInstance<_, PgasClaimError> { PgasClaimError.Unknown(it) }
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun resolveClaimContext(destinationAccountId: AccountId): Result<ClaimContext> = diagnostics.markRegion(RCommon.string.stall_reading_chain_state) {
        runCatching {
            val chain = chainRegistry.assetHub()
            val collection = activePeopleCollectionUseCase.getActivePeopleCollection()
            val period = currentPeriod()
            val previousBalance = currentPgasBalance(destinationAccountId)
            Timber.i("resolved chain=${chain.id}, collection=$collection, period=$period, pre-tx balance=$previousBalance")
            ClaimContext(chain, collection, period, previousBalance)
        }
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun submitClaim(ctx: ClaimContext, slotIndex: UInt, destinationAccountId: AccountId): Result<Unit> = diagnostics.markRegion(RCommon.string.stall_submitting_transaction) {
        Timber.i("picked free slotIndex=$slotIndex; submitting claim_pgas extrinsic")
        pgasOrigins.asPgasClaim(ctx.period, slotIndex, ctx.collection).flatMap { origin ->
            extrinsicService.submitExtrinsicAndAwaitExecution(
                chain = ctx.chain,
                origin = origin,
                submissionFailureRecovery = resubmitWhenValidFactory.create(ctx.chain.id),
            ) {
                pgas.claimPgas(slotIndex, destinationAccountId)
            }
                .flattenExecutionFailure()
                .coerceToUnit()
        }
            .onSuccess { Timber.i("claim_pgas executed for slotIndex=$slotIndex") }
    }

    private suspend fun currentPgasBalance(accountId: AccountId): Balance {
        val asset = pgasChainAssetProvider.asset()
        return tokenBalanceTypeRegistry.typeFor(asset).getBalance(accountId).total
    }

    context(diagnostics: StalenessReportCollector)
    private suspend fun pickFreeSlotIndex(
        chainId: ChainId,
        period: UInt,
        collection: PeopleCollection,
    ): Result<UInt> = diagnostics.markRegion(RCommon.string.pgas_stall_picking_slot) {
        runCatching {
            val maxSlots = pgasRepository.maxClaimsPerPeriod(chainId, collection)
            val tld = dotNsTldProvider.getTldRetrying()
            Timber.i("scanning $maxSlots slots for period=$period")
            val aliasesByIndex = (0u until maxSlots).associateWith { slot ->
                val context = BandersnatchContext.pgasClaim(tld, period, slot)
                bandersnatchKeyResolver.getAliasInContext(collection, context)
            }
            val taken = pgasRepository.claimedAliases(chainId, period, aliasesByIndex.values.toList())
            Timber.i("${taken.size}/$maxSlots slots already claimed")
            aliasesByIndex.entries.first { (_, alias) -> alias !in taken }.key
        }
    }

    private fun currentPeriod(): UInt {
        return (Clock.System.now().epochSeconds / SECONDS_PER_PERIOD).toUInt()
    }

    private data class ClaimContext(
        val chain: Chain,
        val collection: PeopleCollection,
        val period: UInt,
        val previousBalance: Balance,
    )
}
