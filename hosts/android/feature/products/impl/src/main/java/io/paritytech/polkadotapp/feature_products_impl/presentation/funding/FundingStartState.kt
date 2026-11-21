package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingEntry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.toReason
import uniffi.truapi.FundingCancel

/** Where the user's start of the session on the quote on screen stands. */
sealed interface FundingStartState {
    data object Idle : FundingStartState

    /** Waiting for the provider to take the session. */
    data object Starting : FundingStartState

    /** The provider would not take it; the user can start again. */
    data object StartFailed : FundingStartState

    data object Started : FundingStartState
}

/** What the screens show of a session the core holds: whether it started, why it failed, and the user's cancel. */
data class FundingSessionFacts(
    val started: Boolean,
    val failure: FundingFailureReason?,
    /** The user's cancel as the core holds it, once money has moved and the provider decides it. */
    val cancel: FundingCancel?,
) {
    companion object {
        fun of(entry: FundingEntry?) = FundingSessionFacts(
            started = entry?.hasStarted == true,
            failure = entry?.failure?.toReason(),
            cancel = entry?.session?.cancel,
        )
    }
}
