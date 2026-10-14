package io.paritytech.polkadotapp.feature_products_api.domain.pocket

import kotlinx.coroutines.flow.Flow

/** The host-owned card collection. Cards enter only through the host's own approval flow. */
interface PocketCollection {
    fun observeCards(): Flow<List<PocketCard>>

    /**
     * Removes a card on the user's behalf, together with its cached face. Removing a privileged card
     * fails with [PocketRemoveError.Privileged]; removing one that is not held succeeds as
     * [PocketRemoval.ABSENT], which the core asks a host to tell apart from a removal it performed.
     */
    suspend fun removeCard(key: PocketCardKey): Result<PocketRemoval>
}
