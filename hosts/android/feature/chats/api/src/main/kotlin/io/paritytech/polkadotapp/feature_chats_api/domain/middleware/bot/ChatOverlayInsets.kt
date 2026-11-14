package io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * How much of the bottom of the screen the global chat overlays (such as a product's game pill) cover
 * right now, in pixels, above the navigation bar. The overlay host reports it; the chat lifts its input
 * slot by it, so a room footer stays above the pill instead of under it.
 */
object ChatOverlayInsets {
    private val bottomPx = MutableStateFlow(0)

    val bottom: StateFlow<Int> = bottomPx.asStateFlow()

    fun reportBottom(px: Int) {
        bottomPx.value = px.coerceAtLeast(0)
    }
}
