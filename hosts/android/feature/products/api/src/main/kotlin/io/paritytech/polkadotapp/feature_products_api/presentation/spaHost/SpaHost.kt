package io.paritytech.polkadotapp.feature_products_api.presentation.spaHost

import android.webkit.WebView
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.screens.MessageDisplay
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.emptyFlow

/**
 * Factory for self-contained SPA sessions.
 *
 * A SPA session bundles a WebView and the JS↔Kotlin host-API bridge (signing, payments,
 * navigation, storage, …) for one `.dot` product, with inline same-WebView navigation
 * between `.dot` domains. The session lifetime is bound to the supplied [CoroutineScope] —
 * cancelling that scope tears down the bridge and the WebView.
 *
 * Lets features outside `feature/products/impl` host a product without taking on the
 * host-API infrastructure directly.
 */
interface SpaHost {
    context(scope: ComputationalScope, messageDisplay: MessageDisplay)
    /**
     * [underCard] marks the session drawn beneath an expanded Pocket card, whose product may ask
     * the host to cover or uncover the card.
     */
    fun createSession(initialUrl: String, underCard: Boolean = false): SpaHostSession
}

/** A request from the card's product to cover or uncover the card, answered by the screen drawing the card. */
class CardCoverRequest(val covered: Boolean, val reply: CompletableDeferred<CardCoverAnswer>)

/** What the screen answers a [CardCoverRequest] with; a request that reaches no screen is never sent. */
enum class CardCoverAnswer {
    /** The card moved to the requested state. */
    APPLIED,

    /** The user is dragging the face, so it was left alone. */
    USER_MOVING,

    /** The card left before it could move. */
    NOT_PRESENTED,
}

interface SpaHostSession {
    val webView: StateFlow<WebView?>

    /** Current page URL, updated as the product navigates between `.dot` domains. */
    val currentUrl: StateFlow<String>

    /** Page load progress for the hosted product, for a progress indicator. */
    val loadProgress: StateFlow<DotNsLoadProgress>

    /** Whether the page is visible; one waiting for its initial theme is not, even once [loadProgress] completes. */
    val isPageRevealed: StateFlow<Boolean>

    /** Current page title, updated as the product navigates. */
    val title: StateFlow<String>

    /** Cover requests from the product under a card; empty for any other session. */
    val cardCoverRequests: Flow<CardCoverRequest>
        get() = emptyFlow()

    fun pauseConnections()

    fun resumeConnections()
}
