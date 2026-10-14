package io.paritytech.polkadotapp.feature_transaction_storage_api.domain.slotAllocator

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector

interface TransactionStorageSlotAllocator {
    /**
     * Reports its progress into [diagnostics]; callers with no UI attached pass
     * [StalenessReportCollector.NoOp].
     */
    context(diagnostics: StalenessReportCollector)
    suspend fun allocate(target: AccountId, strategy: OnExistingAllocationStrategy): Result<Unit>

    /**
     * Ensures [target] holds an unexpired Bulletin authorization, the condition HOP requires.
     * Calls `HopRuntimeApi.can_account_promote`; when it returns `false`, submits `claim_long_term_storage`
     * and waits until the runtime returns `true`.
     */
    context(diagnostics: StalenessReportCollector)
    suspend fun ensureCanSubmit(target: AccountId): Result<Unit>
}

sealed class TransactionStorageSlotAllocationError(cause: Throwable?) : Throwable(cause) {
    class NoAllocationAvailable(cause: Throwable) : TransactionStorageSlotAllocationError(cause)
    class Unknown(cause: Throwable) : TransactionStorageSlotAllocationError(cause)
}
