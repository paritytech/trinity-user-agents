package io.paritytech.polkadotapp.chains.multiNetwork.chain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.chains.util.planksFromAmount
import java.math.BigDecimal

data class ChainAssetWithAmount(
    val chainAsset: Chain.Asset,
    val amount: Balance,
) {
    override fun toString(): String {
        return "${chainAsset.symbol} ${chainAsset.amountFromPlanks(amount)}"
    }

    fun times(times: BigDecimal): ChainAssetWithAmount {
        return ChainAssetWithAmount(chainAsset, amount * times)
    }
}

fun Balance.withAsset(asset: Chain.Asset): ChainAssetWithAmount {
    return ChainAssetWithAmount(asset, this)
}

fun Chain.Asset.withAmount(amount: Balance): ChainAssetWithAmount {
    return ChainAssetWithAmount(this, amount)
}

fun Chain.Asset.withAmount(amount: BigDecimal): ChainAssetWithAmount {
    return ChainAssetWithAmount(this, amount.planksFromAmount(precision))
}

fun ChainAssetWithAmount.decimalAmount(): BigDecimal {
    return chainAsset.amountFromPlanks(amount)
}
