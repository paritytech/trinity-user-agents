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
import uniffi.truapi.ExpandedCardFaceOutcome
import uniffi.truapi.ExpandedCardTurnOutcome
import uniffi.truapi.PocketCardSide

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
     * the host to show or hide the card face.
     */
    fun createSession(initialUrl: String, underCard: Boolean = false): SpaHostSession
}

/**
 * A request from the product under an expanded card, answered by the screen drawing the card. A
 * request that reaches no screen is never sent.
 */
sealed class ExpandedCardRequest<Outcome : Any>(val notPresented: Outcome) {
    val reply = CompletableDeferred<Outcome>()

    /** Answers that the card left before the request could be carried out. */
    fun answerNotPresented() = reply.complete(notPresented)

    /** Cover the face with the page, or uncover it. */
    class Face(val shown: Boolean) : ExpandedCardRequest<ExpandedCardFaceOutcome>(ExpandedCardFaceOutcome.NOT_PRESENTED)

    /** Turn the card so [side] is up. */
    class Turn(val side: PocketCardSide) : ExpandedCardRequest<ExpandedCardTurnOutcome>(ExpandedCardTurnOutcome.NOT_PRESENTED)
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

    /** Requests from the product under a card; empty for any other session. */
    val expandedCardRequests: Flow<ExpandedCardRequest<*>>
        get() = emptyFlow()

    fun pauseConnections()

    fun resumeConnections()
}
