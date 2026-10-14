package io.paritytech.polkadotapp.feature_pgas_api.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector

interface PgasClaimer {
    /**
     * Reports its progress into [diagnostics]; callers with no UI attached pass
     * [StalenessReportCollector.NoOp].
     */
    context(diagnostics: StalenessReportCollector)
    suspend fun claim(destinationAccountId: AccountId, strategy: OnExistingAllocationStrategy): Result<Unit>
}

sealed class PgasClaimError(cause: Throwable?) : Throwable(cause) {
    class NoAllocationAvailable(cause: Throwable) : PgasClaimError(cause)
    class Unknown(cause: Throwable) : PgasClaimError(cause)
}
