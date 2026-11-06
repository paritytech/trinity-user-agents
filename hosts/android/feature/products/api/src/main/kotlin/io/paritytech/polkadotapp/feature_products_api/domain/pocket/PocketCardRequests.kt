package io.paritytech.polkadotapp.feature_products_api.domain.pocket

import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import javax.inject.Inject
import javax.inject.Singleton

/** A card a link asked to expand, with the [pageQuery] its page opens with. */
data class PocketCardOpenRequest(val key: PocketCardKey, val pageQuery: String)

/** What the Pocket tab is asked to show: a card to expand, or the card list again. */
@Singleton
class PocketCardRequests @Inject constructor() {
    /** A link can arrive before the tab has ever been shown, so the card is held here until the tab finds it in its collection. */
    val requested: StateFlow<PocketCardOpenRequest?>
        field = MutableStateFlow<PocketCardOpenRequest?>(null)

    /** Not held: a close sent while the tab has never been shown has no card to close. */
    val closes: SharedFlow<Unit>
        field = MutableSharedFlow<Unit>(extraBufferCapacity = 1, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    fun requestOpen(key: PocketCardKey, pageQuery: String = "") {
        requested.value = PocketCardOpenRequest(key, pageQuery)
    }

    /** Clears [request] only while it is still the one held, so a newer link is not dropped with it. */
    fun consume(request: PocketCardOpenRequest) {
        requested.compareAndSet(request, null)
    }

    /** Takes Pocket back to its card list from an open card or the Collectibles book, and drops a link still waiting to open. */
    fun requestClose() {
        requested.value = null
        closes.tryEmit(Unit)
    }
}
