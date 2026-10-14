package io.paritytech.polkadotapp.feature_fund_api.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetWithAmount
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.withAmount
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_prices_api.domain.model.FiatAmount
import java.math.BigDecimal

class DepositTerms(
    val minDeposit: Balance,
    val estimatedFee: FiatAmount,
    val conversionRate: ConversionRate,
) {
    class ConversionRate(
        private val from: Chain.Asset,
        private val to: Chain.Asset,
        private val rate: BigDecimal
    ) {
        fun rate() = rate

        fun sampleFrom(): ChainAssetWithAmount {
            return from.withAmount(SAMPLE_CONVERSION_AMOUNT)
        }

        fun sampleTo(): ChainAssetWithAmount {
            val toAmount = rate * SAMPLE_CONVERSION_AMOUNT
            return to.withAmount(toAmount)
        }

        companion object {
            private val SAMPLE_CONVERSION_AMOUNT = BigDecimal.ONE
        }
    }
}
