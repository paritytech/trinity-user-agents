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

// Run by the app's main screen, so a provider's screen opens over whatever is on screen, not from the funding sheet.
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
    // A provider's screen still waiting from before the app's screens were recreated is asked for again.
    suspend fun present() {
        coroutineScope {
            launch { askWaitingFramesAgain() }
            launch { sessions.presentationRequests.collect { router.openFundingOverlay(it.id) } }
            providerFrames.events.collect(::on)
        }
    }

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

    // The core holds the provider's screen until the user taps the notice.
    private suspend fun onFrameRequested(intentId: FundingIntentId, providerId: ProductId) {
        if (router.isFundingOverlayShown(intentId)) {
            providerFrames.open(intentId)
            return
        }

        val provider = interactor.brand(providerId).name
        val text = contextManager.applicationContext.getString(RCommon.string.funding_frame_requested, provider)
        appNotifier.notify(AppNotification.Tappable(text) { scope.launch(dispatchers.main) { show(intentId) } })
    }

    private suspend fun onFrameOpened(request: FundingFrameRequest) {
        frames.add(request)
        router.openFundingProviderFrame(request)
    }

    // A wait that cannot be read gets the notice too: opening it asks the core, which opens nothing when none waits.
    private suspend fun askWaitingFramesAgain() {
        val entries = sessions.entries().firstOrNull().orEmpty()
        for (entry in entries) {
            val providerId = entry.session?.providerId ?: continue
            if (!entry.hasEnded && providerFrames.isWaiting(entry.id).getOrElse { true }) onFrameRequested(entry.id, providerId)
        }
    }

    private suspend fun onFrameClosed(intentId: FundingIntentId) {
        val shown = frames.waiting(intentId)?.isShown == true
        frames.releaseAll(intentId)
        router.closeFundingProviderFrame(intentId)
        if (shown) show(intentId)
    }
}
