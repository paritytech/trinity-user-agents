package io.paritytech.polkadotapp.feature_fund_impl.data.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks

sealed class AutoConversionCheckError : Exception() {
    class QuoteFailure(override val cause: Throwable) : AutoConversionCheckError()

    class FeeCalculationFailure(override val cause: Throwable) : AutoConversionCheckError()

    class NotEnoughBalance(val asset: Chain.Asset, val minRequired: Balance, val got: Balance) : AutoConversionCheckError() {
        override val message: String
            get() = """
                Not enough balance to perform deposit conversion
                Min required: ${asset.amountFromPlanks(minRequired)}
                Got: ${asset.amountFromPlanks(got)}
            """.trimIndent()
    }
}
