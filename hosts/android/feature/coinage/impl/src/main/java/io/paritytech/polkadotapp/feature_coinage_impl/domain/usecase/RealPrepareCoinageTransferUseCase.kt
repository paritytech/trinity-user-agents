package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatRecover
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.common.utils.progressStallReport.markRegion
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.StrategyType
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.CoinageTransactionService
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.PrepareCoinageTransferUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.PreparedTransferMemo
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogD
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogE
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogI
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogW
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.TransferMemoBuilder
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.TransferPlanner
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.TransferPlannerFactory
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.exceptions.InsufficientBalanceException
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.exceptions.TransferSubmissionFailedException
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.ExactMatchStrategyFactory
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.ScheduledTransfer
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.SplitCoinStrategyFactory
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.TransferStrategy
import io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies.UnloadAndSplitVouchersStrategyFactory
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.CoinageAssetSelector
import io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling.SpendScope
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.TransferSubmissionParams
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxStatus
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeoutOrNull
import java.math.BigDecimal
import javax.inject.Inject
import kotlin.time.Duration.Companion.minutes
import kotlin.time.ExperimentalTime
import kotlin.time.Instant
import io.paritytech.polkadotapp.common.R as RCommon

class RealPrepareCoinageTransferUseCase @Inject constructor(
    private val assetSelector: CoinageAssetSelector,
    private val plannerFactory: TransferPlannerFactory,
    private val exactMatchStrategyFactory: ExactMatchStrategyFactory,
    private val splitStrategyFactory: SplitCoinStrategyFactory,
    private val unloadAndSplitStrategyFactory: UnloadAndSplitVouchersStrategyFactory,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val memoBuilder: TransferMemoBuilder,
    private val transactionService: CoinageTransactionService,
) : PrepareCoinageTransferUseCase {
    private companion object {
        /** How long the sender waits for the transactions to reach the wire before calling the send failed. */
        val SUBMISSION_TIMEOUT = 2.minutes
    }

    /**
     * Plans against spendable funds first and only widens if they cannot cover the amount.
     */
    override suspend fun preparePlan(amount: BigDecimal): Result<TransferPlan> {
        val coinsByScope = assetSelector.getSelectableCoinsByScope()
        val vouchersByScope = assetSelector.getSelectableVouchersByScope()

        fun TransferPlanner.planWithin(scope: SpendScope) =
            plan(amount, coinsByScope.getValue(scope), vouchersByScope.getValue(scope))

        return plannerFactory.create()
            .flatMap { planner ->
                planner.planWithin(SpendScope.SPENDABLE)
                    .flatRecover { planner.planWithin(SpendScope.WITH_CONFIRMATION) }
            }
            .onSuccess { coinageLogI("Outgoing TransferPlan: $it") }
            .onFailure { logPlanFailure(amount, it) }
    }

    /**
     * An amount the wallet cannot cover is an ordinary state while the user is still typing, so it does not
     * belong at error level — this same path backs the running plan preview, not just the send.
     */
    private fun logPlanFailure(amount: BigDecimal, error: Throwable) = when (error) {
        is InsufficientBalanceException -> coinageLogD("No transfer plan covers amount: $amount")
        else -> coinageLogE("Failed to construct transfer plan for amount: $amount", error)
    }

    /**
     * The same scheduled path as [prepareScheduledMemo], registered at once, then held until every transaction
     * is on the wire — so the memo only leaves once its coins are on their way.
     */
    @OptIn(ExperimentalTime::class)
    context(diagnostics: StalenessReportCollector)
    override suspend fun prepareMemo(plan: TransferPlan, retryUntil: Instant): Result<PreparedTransferMemo> {
        return createStrategy(plan).schedule(TransferSubmissionParams(buildUntil = retryUntil, retryFailures = true))
            .flatMap { scheduled ->
                // The memo never leaves when its transactions do not, so its coins need not wait for a relaunch.
                submitNow(scheduled)
                    .onFailure { scheduled.handoffCommit.release() }
                    .map { scheduled }
            }
            .flatMap { scheduled ->
                memoBuilder.buildMemo(scheduled.entries)
                    .map { memo -> PreparedTransferMemo(memo, scheduled.handoffCommit) }
            }
            .onSuccess { coinageLogD("TransferMemo built: coins=${it.memo.coins.size}, total=${it.memo.totalValue}") }
    }

    @OptIn(ExperimentalTime::class)
    context(diagnostics: StalenessReportCollector)
    override suspend fun prepareScheduledMemo(plan: TransferPlan, retryUntil: Instant): Result<PreparedTransferMemo> {
        return createStrategy(plan).schedule(TransferSubmissionParams(buildUntil = retryUntil, retryFailures = true))
            .flatMap { scheduled ->
                memoBuilder.buildMemo(scheduled.entries).map { memo ->
                    PreparedTransferMemo(memo, SchedulingHandoffCommit(scheduled, transactionService))
                }
            }
            .onSuccess {
                coinageLogD("Scheduled TransferMemo built: coins=${it.memo.coins.size}, total=${it.memo.totalValue}")
            }
    }

    /**
     * Schedules [scheduled]'s transactions and waits until each one has either been submitted or failed for
     * good. Building runs in the background, so this waits for its outcome rather than doing the work. A wait
     * that runs out fails the send; the transactions stay scheduled and their assets stay recoverable.
     */
    context(diagnostics: StalenessReportCollector)
    private suspend fun submitNow(scheduled: ScheduledTransfer): Result<Unit> =
        transactionService.scheduleTransferTransactions(scheduled).flatMap { groupId ->
            if (groupId == null) {
                Result.success(Unit)
            } else {
                diagnostics.markRegion(RCommon.string.stall_submitting_transaction) {
                    awaitSubmitted(groupId)
                }
            }
        }

    private suspend fun awaitSubmitted(groupId: CoinageOperationGroupId): Result<Unit> {
        val settled = withTimeoutOrNull(SUBMISSION_TIMEOUT) {
            transactionService.subscribeOperationGroupStatuses(groupId)
                .first { states -> states.none { it.status == DurableTxStatus.PENDING_SUBMISSION } }
        }

        return if (settled == null || settled.any { it.status == DurableTxStatus.FAILURE }) {
            coinageLogW("Transfer could not be submitted group=${groupId.value}")
            Result.failure(TransferSubmissionFailedException())
        } else {
            Result.success(Unit)
        }
    }

    private suspend fun createStrategy(plan: TransferPlan): TransferStrategy {
        return when (val strategyType = plan.strategyType) {
            is StrategyType.ExactCoins -> exactMatchStrategyFactory.create(strategyType)
            is StrategyType.Split -> splitStrategyFactory.create(strategyType)
            is StrategyType.UnloadAndSplit -> unloadAndSplitStrategyFactory.create(strategyType, chainAssetProvider.chain())
        }
    }
}

/**
 * The payment's transactions are registered in the same step that makes its handoff final, so they share the
 * transaction that persists the memo: a saved payment always has its transactions, and a lost one never does.
 */
private class SchedulingHandoffCommit(
    private val scheduled: ScheduledTransfer,
    private val transactionService: CoinageTransactionService,
) : CoinageHandoffCommit {
    override suspend fun commit(): Result<Unit> = scheduled.handoffCommit.commit().flatMap {
        transactionService.scheduleTransferTransactions(scheduled).coerceToUnit()
    }

    override suspend fun release(): Result<Unit> = scheduled.handoffCommit.release()
}

/** Returns the group the transactions were scheduled under, or null when there was nothing to schedule. */
private suspend fun CoinageTransactionService.scheduleTransferTransactions(
    transfer: ScheduledTransfer,
): Result<CoinageOperationGroupId?> {
    if (transfer.transactions.isEmpty()) return Result.success(null)

    val groupId = CoinageOperationGroupId.generateNew()

    return scheduleTransactions(transfer.transactions, groupId).map { groupId }
}
