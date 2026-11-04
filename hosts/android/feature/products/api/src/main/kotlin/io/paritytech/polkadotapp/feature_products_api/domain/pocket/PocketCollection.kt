package io.paritytech.polkadotapp.feature_products_api.domain.pocket

import kotlinx.coroutines.flow.Flow

/** The host-owned card collection. Cards enter only through the host's own approval flow. */
interface PocketCollection {
    fun observeCards(): Flow<List<PocketCard>>

    /** Removes a card on the user's behalf, together with its face. A privileged card stays. */
    suspend fun removeCard(key: PocketCardKey): Result<Unit>
}
