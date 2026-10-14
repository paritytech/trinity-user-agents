package io.paritytech.polkadotapp.feature_prices_api.presentation.formatter

import androidx.compose.runtime.staticCompositionLocalOf
import io.paritytech.polkadotapp.design.utils.noLocalProvidedFor
import io.paritytech.polkadotapp.feature_prices_api.presentation.mapper.model.FiatAmountModel

val LocalFiatFormatter = staticCompositionLocalOf<FiatFormatter> {
    noLocalProvidedFor("FiatFormatter")
}

interface FiatFormatter {
    fun formatFiatAmount(
        fiatAmount: FiatAmountModel,
        approx: Boolean = false,
    ): String

    companion object {
        val mocked: FiatFormatter get() = MockedFiatFormatter()
    }
}

private class MockedFiatFormatter : FiatFormatter {
    override fun formatFiatAmount(
        fiatAmount: FiatAmountModel,
        approx: Boolean
    ): String {
        return buildString {
            if (approx) append("≈ ")
            append("${fiatAmount.currencyDisplay}${fiatAmount.fiatAmount}")
        }
    }
}
