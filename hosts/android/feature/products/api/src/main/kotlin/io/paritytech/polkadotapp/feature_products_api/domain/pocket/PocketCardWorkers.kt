package io.paritytech.polkadotapp.feature_products_api.domain.pocket

import kotlinx.coroutines.flow.Flow

/** Worker ownership for Pocket card presentations. */
interface PocketCardWorkers {
    /** Hold the published front until cancellation, and its declared back while [expanded] is true. */
    suspend fun holdVisibleSides(key: PocketCardKey, expanded: Flow<Boolean>)
}
