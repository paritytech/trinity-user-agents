package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

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
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import timber.log.Timber
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.FundingPresentOutcome
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.HostFundingStatusSubscribeItem
import uniffi.truapi.U128
import javax.inject.Inject
import javax.inject.Singleton
import uniffi.truapi.FundingDirection as CoreFundingDirection

/** Serves the core's funding sessions through the app's [FundingOverlay]. */
@Singleton
class AppFundingHostBridge @Inject constructor(
    private val overlay: FundingOverlay,
    private val countryDetector: DeviceCountryDetector,
) : FundingHostBridge {
    private val changedSessions = MutableSharedFlow<String>(
        extraBufferCapacity = EVENT_BUFFER,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
    private val quoteLists = MutableSharedFlow<FundingQuoteListUpdate>(
        extraBufferCapacity = EVENT_BUFFER,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    /** Emits the intent of every session whose status changed. */
    fun changedSessions(): SharedFlow<String> = changedSessions.asSharedFlow()

    /** Emits every quote list the core sends: again as each provider answers, and at every refresh. */
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

        return when (overlay.present(request)) {
            FundingOverlayOutcome.STARTED -> FundingPresentOutcome.STARTED
            FundingOverlayOutcome.DISMISSED -> FundingPresentOutcome.DISMISSED
        }
    }

    override suspend fun presentProviderFrame(providerId: String, intent: String, target: FundingFrameTarget): FundingFrameOutcome {
        val frameTarget = when (target) {
            is FundingFrameTarget.App -> ProviderFrameTarget.App(target.route)
            is FundingFrameTarget.Page -> ProviderFrameTarget.Page(target.url, target.returnUrl)
        }

        return when (val outcome = overlay.presentProviderFrame(ProductId.fromStoredValue(providerId), intent, frameTarget)) {
            ProviderFrameOutcome.Closed -> FundingFrameOutcome.Closed
            ProviderFrameOutcome.Dismissed -> FundingFrameOutcome.Dismissed
            is ProviderFrameOutcome.Returned -> FundingFrameOutcome.Returned(outcome.url)
        }
    }

    override suspend fun detectedCountry(): String? = countryDetector.detect()

    override fun fundingSessionChanged(intent: String, status: HostFundingStatusSubscribeItem) {
        changedSessions.tryEmit(intent)
    }

    override fun fundingQuotesChanged(intent: String, quotes: FundingQuoteList) {
        Timber.tag("truapi.funding").d("quotes %s: best %s, %d rows", intent, quotes.best, quotes.rows.size)
        quoteLists.tryEmit(FundingQuoteListUpdate(intent, quotes))
    }

    private companion object {
        const val EVENT_BUFFER = 64
    }
}

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
