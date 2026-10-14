package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.SwapDirection
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId
import java.math.BigInteger
import kotlin.math.floor

class OmniPool(
    val tokens: Map<HydraDxAssetId, OmniPoolToken>,
)

class OmniPoolFees(
    val protocolFee: Fraction,
    val assetFee: Fraction
)

class OmniPoolToken(
    val hubReserve: BigInteger,
    val shares: BigInteger,
    val protocolShares: BigInteger,
    val tradeability: Tradeability,
    val balance: Balance,
    val fees: OmniPoolFees
)

fun OmniPool.quote(
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

fun OmniPool.calculateOutGivenIn(
    assetIdIn: HydraDxAssetId,
    assetIdOut: HydraDxAssetId,
    amountIn: Balance
): Balance {
    val tokenInState = tokens.getValue(assetIdIn)
    val tokenOutState = tokens.getValue(assetIdOut)

    val protocolFee = tokenInState.fees.protocolFee
    val assetFee = tokenOutState.fees.assetFee

    val inHubReserve = tokenInState.hubReserve.toDouble()
    val inReserve = tokenInState.balance.value.toDouble()

    val inAmount = amountIn.value.toDouble()

    val deltaHubReserveIn = inAmount * inHubReserve / (inReserve + inAmount)

    val protocolFeeAmount = floor(protocolFee.fraction.toDouble() * deltaHubReserveIn)

    val deltaHubReserveOut = deltaHubReserveIn - protocolFeeAmount

    val outReserveHp = tokenOutState.balance.value.toDouble()
    val outHubReserveHp = tokenOutState.hubReserve.toDouble()

    val deltaReserveOut = outReserveHp * deltaHubReserveOut / (outHubReserveHp + deltaHubReserveOut)
    val amountOut = deltaReserveOut.deductFraction(assetFee)

    return amountOut.toBigDecimal().toBigInteger().intoBalance()
}

fun OmniPool.calculateInGivenOut(
    assetIdIn: HydraDxAssetId,
    assetIdOut: HydraDxAssetId,
    amountOut: Balance
): Balance? {
    val tokenInState = tokens.getValue(assetIdIn)
    val tokenOutState = tokens.getValue(assetIdOut)

    val protocolFee = tokenInState.fees.protocolFee
    val assetFee = tokenOutState.fees.assetFee

    val outHubReserve = tokenOutState.hubReserve.toDouble()
    val outReserve = tokenOutState.balance.value.toDouble()

    val outAmount = amountOut.value.toDouble()

    val outReserveNoFee = outReserve.deductFraction(assetFee)

    val deltaHubReserveOut = outHubReserve * outAmount / (outReserveNoFee - outAmount) + 1

    val deltaHubReserveIn = deltaHubReserveOut / (1.0 - protocolFee.fraction.toDouble())

    val inHubReserveHp = tokenInState.hubReserve.toDouble()

    if (deltaHubReserveIn >= inHubReserveHp) {
        return null
    }

    val inReserveHp = tokenInState.balance.value.toDouble()

    val deltaReserveIn = inReserveHp * deltaHubReserveIn / (inHubReserveHp - deltaHubReserveIn) + 1

    return deltaReserveIn.takeIf { it >= 0 }
        ?.toBigDecimal()
        ?.toBigInteger()
        ?.intoBalance()
}

private fun Double.deductFraction(perbill: Fraction): Double = this - this * perbill.fraction.toDouble()
