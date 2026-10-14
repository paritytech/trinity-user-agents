package io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter

import androidx.compose.runtime.staticCompositionLocalOf
import io.paritytech.polkadotapp.design.utils.noLocalProvidedFor
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel

val LocalConversionFormatter = staticCompositionLocalOf<ConversionFormatter> {
    noLocalProvidedFor("ConversionFormatter")
}

interface ConversionFormatter {
    fun formatConversion(
        tokenAmountFrom: TokenAmountModel,
        tokenAmountTo: TokenAmountModel,
        precisionFrom: RoundPrecision,
        precisionTo: RoundPrecision,
        approx: Boolean = false,
    ): String

    companion object {
        val mocked: ConversionFormatter get() = MockedConversionFormatter()
    }
}

private class MockedConversionFormatter : ConversionFormatter {
    override fun formatConversion(
        tokenAmountFrom: TokenAmountModel,
        tokenAmountTo: TokenAmountModel,
        precisionFrom: RoundPrecision,
        precisionTo: RoundPrecision,
        approx: Boolean,
    ): String {
        val formatter = TokenAmountFormatter.mocked
        return buildString {
            append(formatter.formatTokenAmount(tokenAmountFrom, precisionFrom))
            if (approx) append(" ≈ ") else append(" = ")
            append(formatter.formatTokenAmount(tokenAmountTo, precisionTo))
        }
    }
}
