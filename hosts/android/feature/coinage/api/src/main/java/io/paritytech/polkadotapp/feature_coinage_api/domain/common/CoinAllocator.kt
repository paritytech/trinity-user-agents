package io.paritytech.polkadotapp.feature_coinage_api.domain.common

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent

interface CoinAllocator {
    /**
     * Allocates and persists a coin.
     *
     * [provenance] is recorded as given and never derived here: only the caller knows what the coin came out
     * of — a recycler, a split of another coin, or a peer — and that is not recoverable afterwards.
     */
    suspend fun allocate(valueExponent: ValueExponent, provenance: CoinProvenance): Result<Coin>

    /** As [allocate], for coins that share one origin — every coin gets the same [provenance]. */
    suspend fun allocateAll(valueExponents: List<ValueExponent>, provenance: CoinProvenance): Result<List<Coin>>
}
