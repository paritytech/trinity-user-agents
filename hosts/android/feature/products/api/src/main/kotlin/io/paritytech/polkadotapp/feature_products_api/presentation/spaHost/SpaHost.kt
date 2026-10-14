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
     * the host to show or hide the card face.
     */
    fun createSession(initialUrl: String, underCard: Boolean = false): SpaHostSession
}

/** A request from the card's product to show or hide the card face, answered by the screen drawing the card. */
class FaceShownRequest(val shown: Boolean, val reply: CompletableDeferred<FaceShownAnswer>)

/** What the screen answers a [FaceShownRequest] with; a request that reaches no screen is never sent. */
enum class FaceShownAnswer {
    /** The face moved to the requested state. */
    APPLIED,

    /** The user is dragging the face, so it was left alone. */
    USER_MOVING,
}

interface SpaHostSession {
    val webView: StateFlow<WebView?>

    /** Current page URL, updated as the product navigates between `.dot` domains. */
    val currentUrl: StateFlow<String>

    /** Page load progress for the hosted product, for a progress indicator. */
    val loadProgress: StateFlow<DotNsLoadProgress>

    /** Current page title, updated as the product navigates. */
    val title: StateFlow<String>

    /** Face show/hide requests from the product under a card; empty for any other session. */
    val faceShownRequests: Flow<FaceShownRequest>
        get() = emptyFlow()

    fun pauseConnections()

    fun resumeConnections()
}
