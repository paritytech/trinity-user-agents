package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerFungibility
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import kotlinx.coroutines.flow.Flow

/**
 * Everything the user holds, as of one moment: the balance and the coins making it up, read off the same
 * snapshot so a screen showing both cannot show them a tick apart.
 */
data class CoinageHoldings(
    val balance: CoinageBalance,
    val holdings: List<CoinageHolding>,
)

/** One coin or recycler entry the balance counts. */
data class CoinageHolding(
    /** Stable across refreshes, so a coin keeps its identity while the list changes. */
    val id: String,
    val exponent: ValueExponent,
    val value: Balance,
    val keyIndex: CoinageKeyIndex,
    /** Spendable now at no privacy cost; everything else is held back or still arriving. */
    val isReady: Boolean,
    /** Null when the ring it hides in, or how much of that ring was unloaded, is unknown. */
    val recyclerFungibility: RecyclerFungibility?,
    /** Left a recycler as one of a batch, which links it to everything that left with it. */
    val isBatchUnloaded: Boolean,
    /** Payments the coin has been through, as its on-chain age shows; zero for a recycler entry. */
    val hops: Int,
)

interface CoinageHoldingsUseCase {
    /** Re-emits on every change to the user's assets and on every strategy change. */
    fun subscribeHoldings(): Flow<CoinageHoldings>
}
