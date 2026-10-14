package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner.strategies

import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.PlannedMemoEntry
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageScheduledTransactionRequest
import io.paritytech.polkadotapp.feature_coinage_impl.domain.transaction.submission.TransferSubmissionParams

sealed interface TransferStrategy {
    /**
     * Prepares the transfer, owning every mutation (allocation, marking coins spent, marking vouchers used) via
     * [io.paritytech.polkadotapp.feature_coinage_impl.domain.model.CoinageTransaction]. Returns the memo entries
     * describing the coins handed to the recipient, built from the coins actually allocated here.
     *
     * Builds and registers nothing: the transactions come back to be scheduled and built by their policies, as
     * [params] describe.
     */
    context(diagnostics: StalenessReportCollector)
    suspend fun schedule(params: TransferSubmissionParams): Result<ScheduledTransfer>
}

/**
 * [handoffCommit] must be committed once the keys in [entries] are durably on their way — until then the
 * coins are reserved but recoverable, and a relaunch returns them. [transactions] still have to be scheduled.
 */
data class ScheduledTransfer(
    val entries: List<PlannedMemoEntry>,
    val handoffCommit: CoinageHandoffCommit,
    val transactions: List<CoinageScheduledTransactionRequest>,
)
