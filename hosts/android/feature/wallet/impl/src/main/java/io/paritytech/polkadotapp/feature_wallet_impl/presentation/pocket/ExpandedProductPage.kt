package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHostSession
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.job

/**
 * The product hosted under an expanded card. One runs at a time: expanding another card takes the
 * previous one down, and leaving the screen takes the last one with it.
 */
class ExpandedProductPage(
    private val parentScope: CoroutineScope,
    private val openSession: (ComputationalScope, String) -> SpaHostSession,
) {
    private val shownSession = MutableStateFlow<SpaHostSession?>(null)
    private var held: HeldProduct? = null

    val session: StateFlow<SpaHostSession?> = shownSession.asStateFlow()

    /**
     * Shows the product for [url], reusing the one already held when it is the same. A WebView and a
     * page load are what a card costs to open, so a card opened again is worth not paying twice.
     */
    fun open(url: String) {
        val heldProduct = held
        if (heldProduct?.url == url) {
            if (shownSession.value == null) heldProduct.session.resumeConnections()
            shownSession.value = heldProduct.session
            return
        }
        release()

        // A child of the parent, so a screen that goes away takes the WebView with it.
        val scope = CoroutineScope(parentScope.coroutineContext + SupervisorJob(parentScope.coroutineContext.job))
        val product = HeldProduct(url, scope, openSession(ComputationalScope(scope), url))
        held = product
        shownSession.value = product.session
    }

    /** The product stays alive for the card being opened again, paused so it does nothing unseen. */
    fun close() {
        shownSession.value = null
        held?.session?.pauseConnections()
    }

    /**
     * Gives up a product held for a card that [stillCollected] does not recognise, since a card the
     * collection no longer holds has no next tap. Asked only when there is a product to give up.
     */
    fun keepOnly(stillCollected: (String) -> Boolean) {
        val url = held?.url ?: return
        if (!stillCollected(url)) release()
    }

    /**
     * Takes the product down rather than keeping it warm, for a card there is no going back to:
     * one the product removed, or one the collection can no longer read.
     */
    fun release() {
        held?.scope?.cancel()
        held = null
        shownSession.value = null
    }

    private class HeldProduct(val url: String, val scope: CoroutineScope, val session: SpaHostSession)
}
