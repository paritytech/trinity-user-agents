package io.paritytech.polkadotapp.feature_swap_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance

class PathRoughFeeEstimation(val inAssetOut: Balance, val inAssetIn: Balance) {
    companion object {
        fun zero(): PathRoughFeeEstimation {
            return PathRoughFeeEstimation(Balance.ZERO, Balance.ZERO)
        }
    }
}
