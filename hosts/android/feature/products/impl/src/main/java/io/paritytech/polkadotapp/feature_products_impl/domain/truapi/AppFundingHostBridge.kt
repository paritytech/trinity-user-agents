package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.parity.truapi.FundingHostBridge
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlay
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameTarget
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.DeviceCountryDetector
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import timber.log.Timber
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingPresentOutcome
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.HostFundingStatusSubscribeItem
import uniffi.truapi.U128
import javax.inject.Inject
import javax.inject.Singleton
import uniffi.truapi.FundingDirection as CoreFundingDirection

/** Serves the core's funding sessions through the app's [FundingOverlay]. */
@Singleton
class AppFundingHostBridge @Inject constructor(
    // Lazy: the overlay reaches the runtime, whose provider is built with this bridge.
    private val overlay: Lazy<FundingOverlay>,
    private val countryDetector: DeviceCountryDetector,
) : FundingHostBridge {
    private val sessions = MutableStateFlow<Map<String, FundingSessionEntry>>(emptyMap())
    private val options = MutableStateFlow<Map<String, FundingOptions>>(emptyMap())
    private val history = MutableStateFlow<List<FundingSessionSummary>?>(null)
    private val inProgress = MutableStateFlow<Boolean?>(null)
    private val changedSessions = MutableSharedFlow<String>(
        extraBufferCapacity = EVENT_BUFFER,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
    private val quoteLists = MutableSharedFlow<FundingQuoteListUpdate>(
        extraBufferCapacity = EVENT_BUFFER,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    /**
     * Emits the intent of every session whose status changed, once the session is in [sessions]. It never completes
     * and replays nothing, and a collector that falls more than [EVENT_BUFFER] events behind loses the oldest ones,
     * so a collector looks the session up rather than relying on seeing every event.
     */
    fun changedSessions(): SharedFlow<String> = changedSessions.asSharedFlow()

    /** Every session the core pushed and still holds, with its latest progress, by intent. */
    fun sessions(): StateFlow<Map<String, FundingSessionEntry>> = sessions.asStateFlow()

    /** What each open overlay offers, as the core last pushed it, by intent. */
    fun options(): StateFlow<Map<String, FundingOptions>> = options.asStateFlow()

    /** Every recorded session's summary as the core pushed it when funding resumed; `null` until then. */
    fun history(): StateFlow<List<FundingSessionSummary>?> = history.asStateFlow()

    /** Whether a provider still works on any session, as the core last said; `null` until funding resumed. */
    fun inProgress(): StateFlow<Boolean?> = inProgress.asStateFlow()

    /**
     * Emits every quote list the core sends: again as each provider answers, and at every refresh. It never
     * completes and replays nothing; a list a slow collector misses is replaced by the next one.
     */
    fun quoteLists(): SharedFlow<FundingQuoteListUpdate> = quoteLists.asSharedFlow()

    override suspend fun presentFunding(
        productId: String?,
        intent: String,
        direction: CoreFundingDirection,
        amount: U128?,
    ): FundingPresentOutcome {
        val request = FundingOverlayRequest(
            productId = productId?.let { ProductId.fromStoredValue(it) },
            intent = intent,
            direction = direction.toDomain(),
            amount = amount?.toBalance(),
        )

        // The core offers options only while the overlay is up, so they go with it.
        val outcome = try {
            overlay.get().present(request)
        } finally {
            options.update { it - intent }
        }
        return when (outcome) {
            FundingOverlayOutcome.STARTED -> FundingPresentOutcome.STARTED
            FundingOverlayOutcome.DISMISSED -> FundingPresentOutcome.DISMISSED
        }
    }

    override suspend fun presentProviderFrame(providerId: String, intent: String, target: FundingFrameTarget): FundingFrameOutcome {
        val frameTarget = when (target) {
            is FundingFrameTarget.App -> ProviderFrameTarget.App(target.route)
            is FundingFrameTarget.Page -> ProviderFrameTarget.Page(target.url, target.returnUrl)
        }

        return when (val outcome = overlay.get().presentProviderFrame(ProductId.fromStoredValue(providerId), intent, frameTarget)) {
            ProviderFrameOutcome.Closed -> FundingFrameOutcome.Closed
            ProviderFrameOutcome.Dismissed -> FundingFrameOutcome.Dismissed
            is ProviderFrameOutcome.Returned -> FundingFrameOutcome.Returned(outcome.url)
        }
    }

    override suspend fun detectedCountry(): String? = countryDetector.detect()

    override fun fundingSessionChanged(
        intent: String,
        status: HostFundingStatusSubscribeItem,
        session: FundingSession?,
        progress: FundingProgress?,
    ) {
        sessions.update { held ->
            if (session == null) held - intent else held + (intent to FundingSessionEntry(session, progress))
        }
        changedSessions.tryEmit(intent)
    }

    override fun fundingHistory(history: List<FundingSessionSummary>) {
        this.history.value = history
    }

    override fun fundingInProgressChanged(inProgress: Boolean) {
        this.inProgress.value = inProgress
    }

    override fun fundingOptionsChanged(intent: String, options: FundingOptions) {
        this.options.update { it + (intent to options) }
    }

    override fun fundingQuotesChanged(intent: String, quotes: FundingQuoteList) {
        Timber.tag("truapi.funding").d("quotes %s: best %s, %d rows", intent, quotes.best, quotes.rows.size)
        quoteLists.tryEmit(FundingQuoteListUpdate(intent, quotes))
    }

    private companion object {
        const val EVENT_BUFFER = 64
    }
}

/** A session as the core last pushed it, with the progress the host draws for it. */
data class FundingSessionEntry(
    val session: FundingSession,
    val progress: FundingProgress?,
)

data class FundingQuoteListUpdate(
    val intent: String,
    val quotes: FundingQuoteList,
)

internal fun CoreFundingDirection.toDomain(): FundingDirection = when (this) {
    CoreFundingDirection.IN -> FundingDirection.IN
    CoreFundingDirection.OUT -> FundingDirection.OUT
}

internal fun FundingDirection.toCore(): CoreFundingDirection = when (this) {
    FundingDirection.IN -> CoreFundingDirection.IN
    FundingDirection.OUT -> CoreFundingDirection.OUT
}

internal fun U128.toBalance(): Balance = toBigInteger().intoBalance()

internal fun Balance.toU128(): U128 = value.toString()
