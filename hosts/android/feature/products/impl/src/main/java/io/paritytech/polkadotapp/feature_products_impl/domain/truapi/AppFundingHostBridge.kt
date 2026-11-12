package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import dagger.Lazy
import io.parity.truapi.FundingHostBridge
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.CountryDetector
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.RealFundingService
import uniffi.truapi.FundingChangeKind
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.FundingPresentOutcome
import uniffi.truapi.ProductId
import uniffi.truapi.U128
import javax.inject.Inject
import javax.inject.Singleton
import uniffi.truapi.FundingDirection as CoreFundingDirection

/** Serves the core's funding callbacks through the app's one [RealFundingService]. */
@Singleton
class AppFundingHostBridge @Inject constructor(
    // Lazy: the service reaches the runtime, whose provider is built with this bridge.
    private val funding: Lazy<RealFundingService>,
    private val countryDetector: CountryDetector,
) : FundingHostBridge {
    override suspend fun presentFunding(
        productId: ProductId?,
        intent: String,
        direction: CoreFundingDirection,
        amount: U128?,
    ): FundingPresentOutcome = funding.get().present(intent, direction, amount)

    override suspend fun presentProviderFrame(
        providerId: ProductId,
        intent: String,
        target: FundingFrameTarget,
    ): FundingFrameOutcome = funding.get().presentFrame(providerId, intent, target)

    override fun closeProviderFrame(providerId: ProductId, intent: String) {
        funding.get().closeFrames(providerId, intent)
    }

    override suspend fun detectedCountry(): String? = countryDetector.detect()

    override fun fundingChanged(intent: String?, kind: FundingChangeKind) {
        funding.get().changed(intent, kind)
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
