package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.common.di.modules.ProcessScope
import io.paritytech.polkadotapp.common.presentation.notification.AppNotification
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameEvent
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingIntentId
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderFrames
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingSessions
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.FundingFrames
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.firstOrNull
import kotlinx.coroutines.launch
import uniffi.truapi.FundingDirection
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * Shows what the funding parts ask of the app's screens: a new session's sheet, a provider's screen, and the notices
 * around them. The app's main screen runs it, so a provider's screen opens over whatever is on screen rather than from
 * the funding sheet. Also shows the sessions the user comes back to from the CASH card.
 */
@Singleton
class FundingSessionPresenter @Inject constructor(
    private val sessions: FundingSessions,
    private val providerFrames: FundingProviderFrames,
    private val frames: FundingFrames,
    private val router: ProductsRouter,
    private val interactor: FundingFlowInteractor,
    private val appNotifier: AppNotifier,
    private val contextManager: ContextManager,
    // Outlives the screen that asks for a new session, which closes before the sheet for it opens.
    @param:ProcessScope private val scope: CoroutineScope,
    private val dispatchers: CoroutineDispatchers,
) {
    /**
     * Serves the sessions' and provider screens' requests until the caller stops collecting; events wait while nothing does. A provider's
     * screen still waiting from before the app's screens were recreated is asked for again, as when it first asked.
     */
    suspend fun present() {
        coroutineScope {
            launch { askWaitingFramesAgain() }
            launch { sessions.presentationRequests.collect { router.openFundingOverlay(it.id) } }
            providerFrames.events.collect(::on)
        }
    }

    /**
     * A session the core still holds reopens where it stands, with the provider's screen over it while one is open or
     * waits to be; one it has ended opens its detail from the history.
     */
    suspend fun show(intentId: FundingIntentId) {
        val entry = sessions.entryState(intentId).firstOrNull()
        if (entry == null || entry.hasEnded) {
            router.openFundingEnded(intentId)
            return
        }

        if (!router.isFundingOverlayShown(intentId)) router.openFundingOverlay(intentId)
        val presented = frames.waiting(intentId)
        when {
            presented == null -> providerFrames.open(intentId)
            !presented.isShown -> router.openFundingProviderFrame(presented.request)
        }
    }

    /** A new session the same way, from the start. */
    fun startOver(direction: FundingDirection) {
        scope.launch(dispatchers.main) { sessions.open(direction, null).logFailure("Funding could not start over") }
    }

    private suspend fun on(event: FundingFrameEvent) {
        when (event) {
            is FundingFrameEvent.Requested -> onFrameRequested(event.intentId, event.providerId)
            is FundingFrameEvent.Opened -> onFrameOpened(event.request)
            is FundingFrameEvent.Closed -> onFrameClosed(event.intentId)
        }
    }

    /**
     * A provider's screen opens at once while its session's sheet is on top; anywhere else the user is told, and opens
     * it with a tap. The core holds it until then.
     */
    private suspend fun onFrameRequested(intentId: FundingIntentId, providerId: ProductId) {
        if (router.isFundingOverlayShown(intentId)) {
            providerFrames.open(intentId)
            return
        }

        val provider = interactor.brand(providerId).name
        val text = contextManager.applicationContext.getString(RCommon.string.funding_frame_requested, provider)
        appNotifier.notify(AppNotification.Tappable(text) { scope.launch(dispatchers.main) { show(intentId) } })
    }

    /** The core presents the screen the app opened. */
    private suspend fun onFrameOpened(request: FundingFrameRequest) {
        frames.add(request)
        router.openFundingProviderFrame(request)
    }

    /**
     * The core still holds a provider's screen for a session in flight, asked for before the notice was lost. A session
     * whose wait cannot be read gets the notice too: opening it asks the core, which opens nothing when none waits.
     */
    private suspend fun askWaitingFramesAgain() {
        val entries = sessions.entries().firstOrNull().orEmpty()
        for (entry in entries) {
            val providerId = entry.session?.providerId ?: continue
            if (!entry.hasEnded && providerFrames.isWaiting(entry.id).getOrElse { true }) onFrameRequested(entry.id, providerId)
        }
    }

    /** The provider is done with its screens: one on screen gives way to the session it was shown for. */
    private suspend fun onFrameClosed(intentId: FundingIntentId) {
        val shown = frames.waiting(intentId)?.isShown == true
        frames.releaseAll(intentId)
        router.closeFundingProviderFrame(intentId)
        if (shown) show(intentId)
    }
}
