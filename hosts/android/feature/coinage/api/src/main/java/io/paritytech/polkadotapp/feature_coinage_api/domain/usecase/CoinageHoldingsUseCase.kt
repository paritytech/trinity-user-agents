package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclingVerdicts
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinBuckets
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.VoucherBuckets
import kotlinx.coroutines.flow.Flow

/**
 * Everything the user holds, classified, as of one moment.
 *
 * Exposed as a whole rather than as separate balance and item streams so a screen showing both cannot show
 * them a tick apart — the figures and the list beside them are read off the same snapshot, and
 * [io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.coinageBalanceOf] turns this into the
 * former.
 */
data class CoinageHoldings(
    val coins: CoinBuckets,
    /** A coin missing from this has not been judged yet: on its way, and not spendable. */
    val verdicts: RecyclingVerdicts,
    val vouchers: VoucherBuckets,
    /** Whether the selected strategy releases the gaining-privacy bucket once the user confirms. */
    val canSpendWithConfirmation: Boolean,
)

interface CoinageHoldingsUseCase {
    /**
     * Re-emits on every change to the user's assets and on every strategy change, since the strategy is
     * what decides which bucket a coin or voucher falls into.
     */
    fun subscribeHoldings(): Flow<CoinageHoldings>
}
