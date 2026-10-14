package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionSignerSource
import kotlinx.coroutines.flow.Flow
import java.math.BigDecimal
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * Onboards an amount into the recycler as a batch of vouchers signed by a signer, via the
 * `load_recycler_with_external_asset_unpaid` call — fees are free for any account, even empty ones.
 */
interface OnboardingUseCase {
    /**
     * One attempt, reported only as whether it was registered. What becomes of each voucher afterwards is
     * left to the recovery pass, and nothing retries the ones that fail.
     *
     * For callers that onboard the same money again on their own terms — a faucet tap, a deposit the
     * converter re-observes. Anything that must onboard a given sum exactly once belongs on [onboardDurably].
     */
    suspend fun onboard(
        amount: BigDecimal,
        signerSource: TransactionSignerSource.Signed,
    ): Result<Unit>

    /**
     * Onboards [amount], retrying whatever fails until every voucher has finalized or [retryUntil] passes,
     * and reports progress throughout. Fresh vouchers on each attempt — the ledger refuses an output it has
     * already minted.
     *
     * Completion is what says the onboarding is over; the last status is its verdict.
     *
     * [groupId] must be derived from something stable about the operation: a second call with the same id
     * rejoins the vouchers already registered instead of onboarding the amount again, which is what makes a
     * retry after process death safe.
     */
    @OptIn(ExperimentalTime::class)
    fun onboardDurably(
        amount: BigDecimal,
        signerSource: TransactionSignerSource.Signed,
        groupId: CoinageOperationGroupId,
        retryUntil: Instant,
    ): Flow<CoinageTransferDetection>
}
