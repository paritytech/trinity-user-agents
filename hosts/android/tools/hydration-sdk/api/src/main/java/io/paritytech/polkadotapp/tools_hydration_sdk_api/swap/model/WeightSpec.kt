package io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model

import io.paritytech.polkadotapp.common.utils.graph.EdgeWeight

class WeightSpec(
    /**
     * Weight for edges that maintain a fixed rate between two pairs
     */
    val stablePools: EdgeWeight,
    /**
     * Weight for edges that are expected to have deep liquidity
     */
    val highLiquidityPools: EdgeWeight,
    /**
     * Weight for edges that are expected to have shallow liquidity
     */
    val lowLiquidityPools: EdgeWeight
) {
    companion object {
        /**
         * Construct weight spec from the given `baseWeight`. This allows different consumers to
         * inject they weight measure into the hydration swap sdk
         *
         * Note: [baseWeight] should be at least be divisible by 100 as weight spec will use multiple of 10% to deviate weights from base
         */
        fun fromBaseWeight(baseWeight: EdgeWeight): WeightSpec {
            return WeightSpec(
                stablePools = baseWeight.adjustByPercent(-10),
                highLiquidityPools = baseWeight,
                lowLiquidityPools = baseWeight.adjustByPercent(10)
            )
        }

        private fun Int.adjustByPercent(percents: Int): Int {
            return (this * (1 + (percents / 100.0))).toInt()
        }
    }
}
