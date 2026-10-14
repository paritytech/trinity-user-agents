package io.paritytech.polkadotapp.feature_transactions.api.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetWithAmount
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.common.domain.model.AccountId
import java.math.BigDecimal

interface Fee {
    companion object;

    val amount: Balance

    val asset: Chain.Asset
}

/**
 * Fee that is paid by some account, namely [origin]
 */
interface AccountFee : Fee {
    val origin: AccountId
}

fun Fee.toChainAssetWithAmount(): ChainAssetWithAmount {
    return ChainAssetWithAmount(asset, amount)
}

fun List<AccountFee>.totalAmount(chainAsset: Chain.Asset, origin: AccountId): Balance {
    return sumOf { it.getAmount(chainAsset, origin).value }.intoBalance()
}

fun List<Fee>.totalAmount(chainAsset: Chain.Asset): Balance {
    return sumOf { it.getAmount(chainAsset).value }.intoBalance()
}

fun List<Fee>.totalPlanksEnsuringAsset(requireAsset: Chain.Asset): Balance {
    return sumOf {
        require(it.asset.fullId == requireAsset.fullId) {
            "Fees contain fee in different assets: ${it.asset.fullId}"
        }

        it.amount.value
    }.intoBalance()
}

fun AccountFee.getAmount(chainAsset: Chain.Asset, origin: AccountId): Balance {
    return if (asset.fullId == chainAsset.fullId && this.origin == origin) {
        amount
    } else {
        Balance.ZERO
    }
}

fun Fee.decimalAmount(): BigDecimal {
    return asset.amountFromPlanks(amount)
}

fun Fee.getAmount(expectedAsset: Chain.Asset): Balance {
    return if (expectedAsset.fullId == asset.fullId) amount else Balance.ZERO
}
