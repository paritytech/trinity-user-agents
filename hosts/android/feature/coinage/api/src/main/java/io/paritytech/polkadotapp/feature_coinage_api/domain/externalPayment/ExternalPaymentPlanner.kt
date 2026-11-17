package io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment

import io.paritytech.polkadotapp.chains.network.binding.Balance

interface ExternalPaymentPlanner {
    /** How [amount] would be paid out of the wallet as it is now. */
    suspend fun plan(amount: Balance): Result<ExternalPaymentPlan>

    /** Whether [plan] would pay [amount] without spending value still gaining privacy. */
    suspend fun canPayPrivately(amount: Balance): Result<Boolean> = plan(amount).map { it == ExternalPaymentPlan.Private }
}
