package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.aave.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapDirection
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId

data class AavePools(
    val pools: List<AavePool>
) {
    fun quote(
        assetIdIn: HydraDxAssetId,
        assetIdOut: HydraDxAssetId,
        amount: Balance,
        direction: SwapDirection
    ): Balance? {
        val pool = findPool(assetIdIn, assetIdOut) ?: return null

        return pool.quote(assetIdOut, amount, direction)
    }

    private fun findPool(assetIdIn: HydraDxAssetId, assetIdOut: HydraDxAssetId): AavePool? {
        return pools.find { it.canHandleTrade(assetIdIn, assetIdOut) }
    }
}

data class AavePool(
    val reserve: HydraDxAssetId,
    val atoken: HydraDxAssetId,
    val liqudityIn: Balance,
    val liquidityOut: Balance
) {
    fun canHandleTrade(assetIdIn: HydraDxAssetId, assetIdOut: HydraDxAssetId): Boolean {
        return findPoolTokenLiquidity(assetIdIn) != null && findPoolTokenLiquidity(assetIdOut) != null
    }

    fun quote(
        assetIdOut: HydraDxAssetId,
        amount: Balance,
        direction: SwapDirection
    ): Balance? {
        return when (direction) {
            SwapDirection.SPECIFIED_IN -> calculateOutGivenIn(assetIdOut, amount)
            SwapDirection.SPECIFIED_OUT -> calculateInGivenOut(assetIdOut, amount)
        }
    }

    // Here and in calculateInGivenOut we always validate amount out (either specified or calculated) against
    // assetIdOut liquidity since that's the asset that will be removed from the pool
    private fun calculateOutGivenIn(
        assetIdOut: HydraDxAssetId,
        amountIn: Balance,
    ): Balance? {
        val calculatedOut = amountIn
        val liquidityOut = findPoolTokenLiquidity(assetIdOut) ?: return null

        return calculatedOut.takeIf { calculatedOut <= liquidityOut }
    }

    private fun calculateInGivenOut(
        assetIdOut: HydraDxAssetId,
        amountOut: Balance,
    ): Balance? {
        val calculatedIn = amountOut
        val liquidityOut = findPoolTokenLiquidity(assetIdOut) ?: return null

        return calculatedIn.takeIf { amountOut <= liquidityOut }
    }

    private fun findPoolTokenLiquidity(assetId: HydraDxAssetId): Balance? {
        return when (assetId) {
            reserve -> liqudityIn
            atoken -> liquidityOut
            else -> null
        }
    }
}
