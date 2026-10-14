package io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance

sealed class AmountAdjustmentMode {
    /**
     * Transfer exactly amount that user has specified
     */
    object Exact : AmountAdjustmentMode()

    /**
     * Increase amount by estimated fees so received amount matches user input
     */
    class OffsetFees(val feesPaidFromAmount: Balance) : AmountAdjustmentMode()
}
