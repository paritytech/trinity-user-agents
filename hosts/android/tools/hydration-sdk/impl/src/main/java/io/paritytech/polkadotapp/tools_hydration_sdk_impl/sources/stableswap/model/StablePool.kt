package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.stableswap.model

import androidx.annotation.Keep
import com.google.gson.Gson
import com.google.gson.annotations.SerializedName
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.hydra_dx_math.stableswap.StableSwapMathBridge
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapDirection
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.common.HydraDxMathConversions.fromBridgeResultToBalance
import java.math.BigInteger

class StablePool(
    val sharedAsset: StablePoolAsset,
    sharedAssetIssuance: Balance,
    val assets: List<StablePoolAsset>,
    val initialAmplification: BigInteger,
    val finalAmplification: BigInteger,
    val initialBlock: BlockNumber,
    val finalBlock: BlockNumber,
    val currentBlock: BlockNumber,
    fee: Fraction,
    val gson: Gson,
    val pegs: List<List<Balance>>
) {
    companion object {
        fun getDefaultPegs(size: Int): List<List<Balance>> {
            return (0 until size).map {
                listOf(Balance.ONE, Balance.ONE)
            }
        }
    }

    val sharedAssetIssuance = sharedAssetIssuance.value.toString()
    val fee: String = fee.fraction.toPlainString()

    val reserves: String by lazy(LazyThreadSafetyMode.NONE) {
        val reservesInput =
            assets.map { ReservesInput(it.balance.value.toString(), it.id.toInt(), it.decimals) }
        gson.toJson(reservesInput)
    }

    val amplification by lazy(LazyThreadSafetyMode.NONE) {
        calculateAmplification()
    }

    val pegsSerialized: String by lazy(LazyThreadSafetyMode.NONE) {
        val pegsInput = pegs.map { inner -> inner.map { it.value.toString() } }
        gson.toJson(pegsInput)
    }

    private fun calculateAmplification(): String {
        return StableSwapMathBridge.calculate_amplification(
            initialAmplification.toString(),
            finalAmplification.toString(),
            initialBlock.value.toString(),
            finalBlock.value.toString(),
            currentBlock.value.toString()
        )
    }
}

class StablePoolAsset(
    val balance: Balance,
    val id: HydraDxAssetId,
    val decimals: Int
)

fun StablePool.quote(
    assetIdIn: HydraDxAssetId,
    assetIdOut: HydraDxAssetId,
    amount: Balance,
    direction: SwapDirection
): Balance? {
    return when (direction) {
        SwapDirection.SPECIFIED_IN -> calculateOutGivenIn(assetIdIn, assetIdOut, amount)
        SwapDirection.SPECIFIED_OUT -> calculateInGivenOut(assetIdIn, assetIdOut, amount)
    }
}

fun StablePool.calculateOutGivenIn(
    assetIn: HydraDxAssetId,
    assetOut: HydraDxAssetId,
    amountIn: Balance,
): Balance? {
    return when {
        assetIn == sharedAsset.id -> calculateWithdrawOneAsset(assetOut, amountIn)
        assetOut == sharedAsset.id -> calculateShares(assetIn, amountIn)
        else -> calculateOut(assetIn, assetOut, amountIn)
    }
}

fun StablePool.calculateInGivenOut(
    assetIn: HydraDxAssetId,
    assetOut: HydraDxAssetId,
    amountOut: Balance,
): Balance? {
    return when {
        assetOut == sharedAsset.id -> calculateAddOneAsset(assetIn, amountOut)
        assetIn == sharedAsset.id -> calculateSharesForAmount(assetOut, amountOut)
        else -> calculateIn(assetIn, assetOut, amountOut)
    }
}

private fun StablePool.calculateAddOneAsset(
    assetIn: HydraDxAssetId,
    amountOut: Balance,
): Balance? {
    return StableSwapMathBridge.calculate_add_one_asset(
        reserves,
        amountOut.value.toString(),
        assetIn.toInt(),
        amplification,
        sharedAssetIssuance,
        fee,
        pegsSerialized
    ).fromBridgeResultToBalance()
}

private fun StablePool.calculateSharesForAmount(
    assetOut: HydraDxAssetId,
    amountOut: Balance,
): Balance? {
    return StableSwapMathBridge.calculate_shares_for_amount(
        reserves,
        assetOut.toInt(),
        amountOut.value.toString(),
        amplification,
        sharedAssetIssuance,
        fee,
        pegsSerialized
    ).fromBridgeResultToBalance()
}

private fun StablePool.calculateIn(
    assetIn: HydraDxAssetId,
    assetOut: HydraDxAssetId,
    amountOut: Balance,
): Balance? {
    return StableSwapMathBridge.calculate_in_given_out(
        reserves,
        assetIn.toInt(),
        assetOut.toInt(),
        amountOut.value.toString(),
        amplification,
        fee,
        pegsSerialized
    ).fromBridgeResultToBalance()
}

private fun StablePool.calculateWithdrawOneAsset(
    assetOut: HydraDxAssetId,
    amountIn: Balance,
): Balance? {
    return StableSwapMathBridge.calculate_liquidity_out_one_asset(
        reserves,
        amountIn.value.toString(),
        assetOut.toInt(),
        amplification,
        sharedAssetIssuance,
        fee,
        pegsSerialized
    ).fromBridgeResultToBalance()
}

private fun StablePool.calculateShares(
    assetIn: HydraDxAssetId,
    amountIn: Balance,
): Balance? {
    val assets = listOf(SharesAssetInput(assetIn.toInt(), amountIn.value.toString()))
    val assetsJson = gson.toJson(assets)

    return StableSwapMathBridge.calculate_shares(
        reserves,
        assetsJson,
        amplification,
        sharedAssetIssuance,
        fee,
        pegsSerialized
    ).fromBridgeResultToBalance()
}

private fun StablePool.calculateOut(
    assetIn: HydraDxAssetId,
    assetOut: HydraDxAssetId,
    amountIn: Balance,
): Balance? {
    return StableSwapMathBridge.calculate_out_given_in(
        this.reserves,
        assetIn.toInt(),
        assetOut.toInt(),
        amountIn.value.toString(),
        amplification,
        fee,
        pegsSerialized
    ).fromBridgeResultToBalance()
}

@Keep
private class SharesAssetInput(
    @SerializedName("asset_id")
    val assetId: Int,
    val amount: String
)

@Keep
private class ReservesInput(
    val amount: String,
    @SerializedName("asset_id")
    val id: Int,
    val decimals: Int
)
