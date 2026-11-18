package io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment

import io.paritytech.polkadotapp.chains.network.binding.Balance

/** Whether a payment out of the wallet can be made, and what it costs in privacy. */
sealed interface ExternalPaymentPlan {
    /** Payable from value that has gained its privacy. */
    data object Private : ExternalPaymentPlan

    /** Payable only by spending value still gaining privacy, which the user confirms first. */
    data object SpendsGainingPrivacy : ExternalPaymentPlan

    /** The wallet holds only [available], less than the amount. */
    data class NotEnoughAmount(val available: Balance) : ExternalPaymentPlan
}
