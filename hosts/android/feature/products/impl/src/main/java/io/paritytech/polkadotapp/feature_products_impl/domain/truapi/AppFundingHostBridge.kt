package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.parity.truapi.FundingHostBridge
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.CountryDetector
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingIntentId
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.RealFundingProviderFrames
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.RealFundingQuotes
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.RealFundingSessions
import uniffi.truapi.FundingChangeKind
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.FundingPresentOutcome
import uniffi.truapi.ProductId
import uniffi.truapi.U128
import javax.inject.Inject
import javax.inject.Singleton
import uniffi.truapi.FundingDirection as CoreFundingDirection

/** Routes the core's funding callbacks to the part that owns each: the sessions, their quotes, the provider's screens. */
@Singleton
class AppFundingHostBridge @Inject constructor(
    // Lazy: each part reaches the runtime, whose provider is built with this bridge.
    private val sessions: Lazy<RealFundingSessions>,
    private val quotes: Lazy<RealFundingQuotes>,
    private val providerFrames: Lazy<RealFundingProviderFrames>,
    private val countryDetector: CountryDetector,
) : FundingHostBridge {
    override suspend fun presentFunding(
        productId: ProductId?,
        intent: String,
        direction: CoreFundingDirection,
        amount: U128?,
    ): FundingPresentOutcome = sessions.get().present(FundingIntentId(intent), direction, amount?.toBalanceOrNull())

    override fun providerFrameRequested(providerId: ProductId, intent: String) {
        providerFrames.get().requested(providerId, FundingIntentId(intent))
    }

    override suspend fun presentProviderFrame(
        providerId: ProductId,
        intent: String,
        target: FundingFrameTarget,
    ): FundingFrameOutcome = providerFrames.get().present(providerId, FundingIntentId(intent), target)

    override fun closeProviderFrame(providerId: ProductId, intent: String) {
        providerFrames.get().close(providerId, FundingIntentId(intent))
    }

    override suspend fun detectedCountry(): String? = countryDetector.detect()

    override fun fundingChanged(intent: String?, kind: FundingChangeKind) {
        val intentId = intent?.let(::FundingIntentId) ?: return
        when (kind) {
            FundingChangeKind.SESSION -> sessions.get().onSessionChanged(intentId)
            FundingChangeKind.OPTIONS -> sessions.get().onOptionsChanged(intentId)
            FundingChangeKind.QUOTES -> quotes.get().onQuotesChanged(intentId)
            FundingChangeKind.PROVIDERS -> Unit
        }
    }
}

internal fun CoreFundingDirection.toDomain(): FundingDirection = when (this) {
    CoreFundingDirection.IN -> FundingDirection.IN
    CoreFundingDirection.OUT -> FundingDirection.OUT
}

internal fun FundingDirection.toCore(): CoreFundingDirection = when (this) {
    FundingDirection.IN -> CoreFundingDirection.IN
    FundingDirection.OUT -> CoreFundingDirection.OUT
}
