package io.paritytech.polkadotapp.feature_products_api.domain.pocket

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject
import javax.inject.Singleton

/**
 * The card a link asked the Pocket tab to expand. A link can arrive before the tab has ever been
 * shown, so the request is held here until the tab finds that card in its collection.
 */
@Singleton
class PocketCardOpenRequests @Inject constructor() {
    val requested: StateFlow<PocketCardKey?>
        field = MutableStateFlow<PocketCardKey?>(null)

    fun request(key: PocketCardKey) {
        requested.value = key
    }

    /** Clears the request only while it still names [key], so a newer link is not dropped with it. */
    fun consume(key: PocketCardKey) {
        requested.compareAndSet(key, null)
    }
}
