package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.model

import io.paritytech.polkadotapp.chains.network.binding.Balance

internal data class CrossChainWeightResult(
    val paidByAccount: Balance,
    val paidFromHolding: Balance
) {
    companion object
}

internal fun CrossChainWeightResult.paidByAccountOrNull(): Balance? {
    return paidByAccount.takeIf { paidByAccount.isPositive() }
}

internal fun CrossChainWeightResult.Companion.zero() = CrossChainWeightResult(Balance.ZERO, Balance.ZERO)

internal fun CrossChainWeightResult?.orZero() = this ?: CrossChainWeightResult.zero()
