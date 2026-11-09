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
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.DeviceCountryDetector
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.truapi.FundingChangeKind
import uniffi.truapi.FundingFrameContext
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.FundingPresentOutcome
import uniffi.truapi.ProductId
import uniffi.truapi.U128
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.seconds
import uniffi.truapi.FundingDirection as CoreFundingDirection

/**
 * Serves the core's funding sessions through the app's [FundingOverlay]. The core keeps every session, option and
 * quote list; a change notice goes to [TrUAPIFundingRuntime], which reads the new state back.
 */
@Singleton
class AppFundingHostBridge @Inject constructor(
    // Lazy: the overlay reaches the runtime, whose provider is built with this bridge.
    private val overlay: Lazy<FundingOverlay>,
    private val funding: Lazy<TrUAPIFundingRuntime>,
    private val countryDetector: DeviceCountryDetector,
    private val resolveProduct: ResolveProductUseCase,
) : FundingHostBridge {
    override suspend fun presentFunding(
        productId: ProductId?,
        intent: String,
        direction: CoreFundingDirection,
        amount: U128?,
    ): FundingPresentOutcome {
        val request = FundingOverlayRequest(
            productId = productId,
            intent = intent,
            direction = direction.toDomain(),
            amount = amount?.toBalance(),
        )
        return when (overlay.get().present(request)) {
            FundingOverlayOutcome.STARTED -> FundingPresentOutcome.STARTED
            FundingOverlayOutcome.DISMISSED -> FundingPresentOutcome.DISMISSED
        }
    }

    override suspend fun presentProviderFrame(
        providerId: ProductId,
        intent: String,
        target: FundingFrameTarget,
        context: FundingFrameContext,
    ): FundingFrameOutcome {
        val frameTarget = when (target) {
            // The core checked the route, which is the path inside the App.
            is FundingFrameTarget.App -> ProviderFrameTarget.App(target.route)
            is FundingFrameTarget.Page -> ProviderFrameTarget.Page(target.url, target.returnUrl)
        }
        return when (val outcome = overlay.get().presentProviderFrame(providerId, intent, frameTarget, context.awaitsUserTransfer)) {
            ProviderFrameOutcome.Closed -> FundingFrameOutcome.Closed
            ProviderFrameOutcome.Dismissed -> FundingFrameOutcome.Dismissed
            is ProviderFrameOutcome.Returned -> FundingFrameOutcome.Returned(outcome.url)
        }
    }

    override fun closeProviderFrame(providerId: ProductId, intent: String) {
        overlay.get().closeProviderFrame(providerId, intent)
    }

    // Asked only for a published provider: a bundled one's entry says whether it ships an App. Its App comes from
    // dotNS, as the frame opens it; one that cannot be resolved in time gets its App route through, so the frame fails
    // as it did before.
    override suspend fun providerHasApp(providerId: ProductId): Boolean {
        val resolved = withTimeoutOrNull(APP_LOOKUP_TIMEOUT) { resolveProduct.resolve(providerId) }
            ?: return true
        resolved.exceptionOrNull()?.let { if (it is CancellationException) throw it }
        return resolved.map { it.executables.app != null }.getOrDefault(true)
    }

    override suspend fun detectedCountry(): String? = countryDetector.detect()

    override fun fundingChanged(intent: String?, kind: FundingChangeKind) {
        funding.get().onChanged(intent, kind)
    }

    private companion object {
        val APP_LOOKUP_TIMEOUT = 10.seconds
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

internal fun U128.toBalance(): Balance = toBigInteger().intoBalance()

internal fun Balance.toU128(): U128 = value.toString()
