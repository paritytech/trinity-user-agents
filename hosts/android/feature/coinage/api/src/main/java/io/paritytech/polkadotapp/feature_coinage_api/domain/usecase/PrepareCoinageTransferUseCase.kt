package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferMemo
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import java.math.BigDecimal
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

interface PrepareCoinageTransferUseCase {
    suspend fun preparePlan(amount: BigDecimal): Result<TransferPlan>

    /**
     * Builds the memo and reserves the coins it names. The reservation is provisional: commit
     * [PreparedTransferMemo.handoffCommit] once the memo is durably on its way to the recipient, or leave it
     * and a relaunch returns the coins.
     *
     * The memo's transactions are submitted before it is returned, and built again should an attempt be proven
     * unable to land — until [retryUntil] has passed with their inputs gone from the chain.
     *
     * Reports its progress into [diagnostics]; callers with no UI attached pass
     * [StalenessReportCollector.NoOp].
     */
    @OptIn(ExperimentalTime::class)
    context(diagnostics: StalenessReportCollector)
    suspend fun prepareMemo(plan: TransferPlan, retryUntil: Instant): Result<PreparedTransferMemo>

    /**
     * Builds the memo and reserves its coins like [prepareMemo], without building or submitting any
     * transaction: they are built afterwards, in the background, and retried the same way.
     *
     * [PreparedTransferMemo.handoffCommit] also registers those transactions, so it must run inside the
     * database transaction that persists the memo: a crash then either loses both or keeps both.
     */
    @OptIn(ExperimentalTime::class)
    context(diagnostics: StalenessReportCollector)
    suspend fun prepareScheduledMemo(plan: TransferPlan, retryUntil: Instant): Result<PreparedTransferMemo>
}

data class PreparedTransferMemo(
    val memo: TransferMemo,
    val handoffCommit: CoinageHandoffCommit,
)

@OptIn(ExperimentalTime::class)
context(diagnostics: StalenessReportCollector)
suspend fun PrepareCoinageTransferUseCase.prepareMemo(amount: BigDecimal, retryUntil: Instant) =
    preparePlan(amount).flatMap { prepareMemo(it, retryUntil) }

@OptIn(ExperimentalTime::class)
context(diagnostics: StalenessReportCollector)
suspend fun PrepareCoinageTransferUseCase.prepareScheduledMemo(amount: BigDecimal, retryUntil: Instant) =
    preparePlan(amount).flatMap { prepareScheduledMemo(it, retryUntil) }
