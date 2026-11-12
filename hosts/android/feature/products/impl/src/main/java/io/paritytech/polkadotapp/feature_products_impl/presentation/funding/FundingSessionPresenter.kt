package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.common.presentation.notification.AppNotification
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingEvent
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingService
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingSessionScreens
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.WorkerCrashNotice
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.frame.FundingFrames
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.firstOrNull
import kotlinx.coroutines.launch
import uniffi.truapi.FundingDirection
import javax.inject.Inject
import javax.inject.Singleton
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * Shows what the funding service asks of the app's screens: a new session's sheet, a provider's screen, and the notices
 * around them. The app's main screen runs it, so a provider's screen opens over whatever is on screen rather than from
 * the funding sheet. Also shows the sessions the user comes back to from the CASH card.
 */
@Singleton
class FundingSessionPresenter @Inject constructor(
    private val service: FundingService,
    private val frames: FundingFrames,
    private val router: ProductsRouter,
    private val interactor: FundingFlowInteractor,
    private val appNotifier: AppNotifier,
    private val contextManager: ContextManager,
    private val workerCrashNotice: WorkerCrashNotice,
    dispatchers: CoroutineDispatchers,
) : FundingSessionScreens {
    // Outlives the screen that asks for a new session, which closes before the sheet for it opens.
    private val scope = CoroutineScope(SupervisorJob() + dispatchers.main)

    /** Serves the service's events until the caller stops collecting; events wait while nothing does. */
    override suspend fun present() {
        service.events.collect(::on)
    }

    /**
     * A session the core still holds reopens where it stands, with the provider's screen over it while one waits; one
     * it has ended opens its detail from the history.
     */
    override suspend fun show(intent: String) {
        val entry = service.entryState(intent).firstOrNull()
        if (entry == null || entry.hasEnded) {
            router.openFundingEnded(intent)
            return
        }

        if (!router.isFundingOverlayShown(intent)) router.openFundingOverlay(intent)
        frames.waiting(intent)?.takeIf { !it.isShown }?.let { open(it.request) }
    }

    /** A new session the same way, from the start. */
    fun startOver(direction: FundingDirection) {
        scope.launch { service.open(direction, null).logFailure("Funding could not start over") }
    }

    private suspend fun on(event: FundingEvent) {
        when (event) {
            is FundingEvent.PresentationRequested -> router.openFundingOverlay(event.entry.id)
            is FundingEvent.FrameRequested -> onFrameRequested(event.request)
            is FundingEvent.FrameClosed -> onFrameClosed(event.entryId)
            is FundingEvent.ProviderNotWorking -> workerCrashNotice.onCrashing(event.productId)
        }
    }

    /** A provider's screen opens at once over its session's sheet; anywhere else the user is told, and opens it with a tap. */
    private suspend fun onFrameRequested(request: FundingFrameRequest) {
        frames.add(request)
        // TODO(#2057): once the core holds a provider's screen until the host opens it, this request arrives through
        //  `provider_frame_requested` before any frame, and `open` lets the core present it.
        if (router.isFundingOverlayShown(request.entryId)) {
            open(request)
            return
        }

        val provider = interactor.brand(request.providerId).name
        val text = contextManager.applicationContext.getString(RCommon.string.funding_frame_requested, provider)
        appNotifier.notify(AppNotification.Tappable(text) { scope.launch { show(request.entryId) } })
    }

    private suspend fun open(request: FundingFrameRequest) {
        service.openProviderFrame(request)
        router.openFundingProviderFrame(request.entryId, request.id)
    }

    /** The provider is done with its screens: one on screen gives way to the session it was shown for. */
    private suspend fun onFrameClosed(entryId: String) {
        val shown = frames.waiting(entryId)?.isShown == true
        frames.releaseAll(entryId)
        router.closeFundingProviderFrame(entryId)
        if (shown) show(entryId)
    }
}
