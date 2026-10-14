package io.paritytech.polkadotapp.feature_swap_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance

/**
 * Deductions from account balance other than those caused by fees
 */
class SwapMaxAdditionalAmountDeduction(
    val fromCountedTowardsEd: Balance
)
