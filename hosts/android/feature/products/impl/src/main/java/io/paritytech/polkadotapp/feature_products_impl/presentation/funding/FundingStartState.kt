package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingEntry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.toReason
import uniffi.truapi.FundingCancel
import uniffi.truapi.ProductId

sealed interface FundingStartState {
    data object Idle : FundingStartState

    data object Starting : FundingStartState

    data object StartFailed : FundingStartState

    data object Started : FundingStartState
}

data class FundingSessionFacts(
    val started: Boolean,
    val failure: FundingFailureReason?,
    val cancel: FundingCancel?,
    val providerId: ProductId?,
) {
    companion object {
        fun of(entry: FundingEntry?) = FundingSessionFacts(
            started = entry?.hasStarted == true,
            failure = entry?.failure?.toReason(),
            cancel = entry?.session?.cancel,
            providerId = entry?.session?.providerId,
        )
    }
}
