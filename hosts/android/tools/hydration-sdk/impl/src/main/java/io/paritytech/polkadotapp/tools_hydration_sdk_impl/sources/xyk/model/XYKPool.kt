package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.xyk.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.hydra_dx_math.xyk.HYKSwapMathBridge
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapDirection
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.common.HydraDxMathConversions.fromBridgeResultToBalance

class XYKPools(
    val fees: XYKFees,
    val pools: List<XYKPool>
) {
    fun quote(
        poolAddress: AccountId,
        assetIdIn: HydraDxAssetId,
        assetIdOut: HydraDxAssetId,
        amount: Balance,
        direction: SwapDirection
    ): Balance? {
        val relevantPool = pools.first { it.address == poolAddress }

        return relevantPool.quote(assetIdIn, assetIdOut, amount, direction, fees)
    }
}

class XYKPool(
    val address: AccountId,
    val firstAsset: XYKPoolAsset,
    val secondAsset: XYKPoolAsset,
) {
    fun getAsset(assetId: HydraDxAssetId): XYKPoolAsset {
        return when {
            firstAsset.id == assetId -> firstAsset
            secondAsset.id == assetId -> secondAsset
            else -> error("Unknown asset for the pool")
        }
    }
}

class XYKPoolAsset(
    val balance: Balance,
    val id: HydraDxAssetId,
)

fun XYKPool.quote(
    assetIdIn: HydraDxAssetId,
    assetIdOut: HydraDxAssetId,
    amount: Balance,
    direction: SwapDirection,
    fees: XYKFees
): Balance? {
    return when (direction) {
        SwapDirection.SPECIFIED_IN -> calculateOutGivenIn(assetIdIn, assetIdOut, amount, fees)
        SwapDirection.SPECIFIED_OUT -> calculateInGivenOut(assetIdIn, assetIdOut, amount, fees)
    }
}

private fun XYKPool.calculateOutGivenIn(
    assetIdIn: HydraDxAssetId,
    assetIdOut: HydraDxAssetId,
    amountIn: Balance,
    feesConfig: XYKFees
): Balance? {
    val assetIn = getAsset(assetIdIn)
    val assetOut = getAsset(assetIdOut)

    val amountOut = HYKSwapMathBridge.calculate_out_given_in(
        assetIn.balance.value.toString(),
        assetOut.balance.value.toString(),
        amountIn.value.toString()
    ).fromBridgeResultToBalance() ?: return null

    val fees = feesConfig.feeFrom(amountOut) ?: return null

    return (amountOut - fees).atLeastZero()
}

private fun XYKPool.calculateInGivenOut(
    assetIdIn: HydraDxAssetId,
    assetIdOut: HydraDxAssetId,
    amountOut: Balance,
    feesConfig: XYKFees,
): Balance? {
    val assetIn = getAsset(assetIdIn)
    val assetOut = getAsset(assetIdOut)

    val amountIn = HYKSwapMathBridge.calculate_in_given_out(
        assetIn.balance.value.toString(),
        assetOut.balance.value.toString(),
        amountOut.value.toString()
    ).fromBridgeResultToBalance() ?: return null

    val fees = feesConfig.feeFrom(amountIn) ?: return null

    return amountIn + fees
}

private fun XYKFees.feeFrom(amount: Balance): Balance? {
    return HYKSwapMathBridge.calculate_pool_trade_fee(amount.value.toString(), nominator.toString(), denominator.toString())
        .fromBridgeResultToBalance()
}
