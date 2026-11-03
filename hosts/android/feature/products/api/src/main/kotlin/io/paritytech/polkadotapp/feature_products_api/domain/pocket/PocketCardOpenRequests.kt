package io.paritytech.polkadotapp.feature_products_api.domain.pocket

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject
import javax.inject.Singleton

/** A card a link asked to expand, with the [pageQuery] its page opens with. */
data class PocketCardOpenRequest(val key: PocketCardKey, val pageQuery: String)

/**
 * The card a link asked the Pocket tab to expand. A link can arrive before the tab has ever been
 * shown, so the request is held here until the tab finds that card in its collection.
 */
@Singleton
class PocketCardOpenRequests @Inject constructor() {
    val requested: StateFlow<PocketCardOpenRequest?>
        field = MutableStateFlow<PocketCardOpenRequest?>(null)

    fun request(key: PocketCardKey, pageQuery: String = "") {
        requested.value = PocketCardOpenRequest(key, pageQuery)
    }

    /** Clears [request] only while it is still the one held, so a newer link is not dropped with it. */
    fun consume(request: PocketCardOpenRequest) {
        requested.compareAndSet(request, null)
    }
}
