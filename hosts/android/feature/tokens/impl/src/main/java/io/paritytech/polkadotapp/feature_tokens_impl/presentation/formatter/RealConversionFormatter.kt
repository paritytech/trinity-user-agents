package io.paritytech.polkadotapp.feature_tokens_impl.presentation.formatter

import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.ConversionFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealConversionFormatter @Inject constructor(
    private val formatter: TokenAmountFormatter,
) : ConversionFormatter {
    override fun formatConversion(
        tokenAmountFrom: TokenAmountModel,
        tokenAmountTo: TokenAmountModel,
        precisionFrom: RoundPrecision,
        precisionTo: RoundPrecision,
        approx: Boolean,
    ): String {
        return buildString {
            append(formatter.formatTokenAmount(tokenAmountFrom, precisionFrom))
            if (approx) append(" ≈ ") else append(" = ")
            append(formatter.formatTokenAmount(tokenAmountTo, precisionTo))
        }
    }
}
