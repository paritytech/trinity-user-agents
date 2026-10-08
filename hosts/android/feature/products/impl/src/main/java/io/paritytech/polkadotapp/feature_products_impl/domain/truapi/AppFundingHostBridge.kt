package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.FundingHostBridge
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlay
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayRequest
import io.paritytech.polkadotapp.feature_products_api.domain.funding.ProviderFrameOutcome
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import timber.log.Timber
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingPresentOutcome
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.HostFundingStatusSubscribeItem
import uniffi.truapi.U128
import javax.inject.Inject
import javax.inject.Singleton
import uniffi.truapi.FundingDirection as CoreFundingDirection

/** Serves the core's funding sessions through the app's [FundingOverlay]. */
@Singleton
class AppFundingHostBridge @Inject constructor(
    private val overlay: FundingOverlay,
) : FundingHostBridge {
    private val sessionChanges = MutableSharedFlow<Unit>(
        extraBufferCapacity = 1,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    /** Emits whenever a session's status changes. */
    fun sessionChanges(): SharedFlow<Unit> = sessionChanges.asSharedFlow()

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

    override suspend fun presentProviderFrame(providerId: String, intent: String, route: String): FundingFrameOutcome =
        when (overlay.presentProviderFrame(ProductId.fromStoredValue(providerId), intent, route)) {
            ProviderFrameOutcome.CLOSED -> FundingFrameOutcome.CLOSED
            ProviderFrameOutcome.DISMISSED -> FundingFrameOutcome.DISMISSED
        }

    override fun fundingSessionChanged(intent: String, status: HostFundingStatusSubscribeItem) {
        sessionChanges.tryEmit(Unit)
    }

    override fun fundingQuoteChanged(intent: String, row: FundingQuoteRow) {
        Timber.tag("truapi.funding").d("quote %s: %s %s", intent, row.providerId, row.state)
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
