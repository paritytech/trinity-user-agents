package io.paritytech.polkadotapp.feature_coinage_api.domain.model

import java.math.BigDecimal

data class TransferPlan(
    val strategyType: StrategyType
)

data class PlannedMemoEntry(
    val coinDerivationIndex: CoinageKeyIndex,
    val valueExponent: ValueExponent
)

sealed interface StrategyType {
    data class ExactCoins(
        val coins: List<Coin>
    ) : StrategyType

    data class Split(
        val splitFrom: Coin,
        val recipientDenominations: List<ValueExponent>,
        val changeDenominations: List<ValueExponent>,
        val exactCoins: List<Coin>
    ) : StrategyType

    data class UnloadAndSplit(
        val vouchersToUnload: List<RecyclerVoucher>,
        val recipientAmount: BigDecimal,
        val exactCoins: List<Coin>
    ) : StrategyType
}
