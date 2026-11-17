package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageExportId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferMemo
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.TransferPlan
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageHandoffCommit
import java.math.BigDecimal
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

interface PrepareCoinageTransferUseCase {
    suspend fun preparePlan(amount: BigDecimal): Result<TransferPlan>

    /**
     * Hands coins worth the plan's amount out of the wallet under [exportId], with their keys in the memo. The
     * handout is provisional: commit [PreparedTransferMemo.handoffCommit] before the memo is made durable anywhere,
     * or release it when the payment is abandoned; a relaunch releases one never committed.
     *
     * The coins that still need minting are built once the handout is committed, and built again until [retryUntil].
     * [exportId] must be stable for a payment that can be retried, so a second call joins the first.
     */
    @OptIn(ExperimentalTime::class)
    suspend fun prepareMemo(plan: TransferPlan, exportId: CoinageExportId, retryUntil: Instant): Result<PreparedTransferMemo>
}

data class PreparedTransferMemo(
    val exportId: CoinageExportId,
    val memo: TransferMemo,
    val handoffCommit: CoinageHandoffCommit,
)

@OptIn(ExperimentalTime::class)
suspend fun PrepareCoinageTransferUseCase.prepareMemo(amount: BigDecimal, exportId: CoinageExportId, retryUntil: Instant) =
    preparePlan(amount).flatMap { prepareMemo(it, exportId, retryUntil) }
