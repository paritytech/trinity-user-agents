package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import uniffi.truapi.FundingCancel

/** Whether the session has been handed to a provider, and why it failed once it has. */
data class FundingStartState(
    val isStarting: Boolean,
    val failed: Boolean,
    val started: Boolean,
    val failure: FundingFailureReason?,
    val isCancelling: Boolean,
    /** The user's cancel as the core holds it, once money has moved and the provider decides it. */
    val cancel: FundingCancel?,
)
