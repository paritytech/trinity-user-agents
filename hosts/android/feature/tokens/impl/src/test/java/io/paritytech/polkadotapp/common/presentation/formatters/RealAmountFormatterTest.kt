package io.paritytech.polkadotapp.common.presentation.formatters

import io.paritytech.polkadotapp.common.presentation.formatters.number.RealNumberFormatterFactory
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrandProvider
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenSymbolAppearance
import io.paritytech.polkadotapp.feature_tokens_impl.presentation.formatter.RealTokenAmountFormatter
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertEquals
import org.junit.Test

private const val PUBLISHED_SYMBOL = "CASH"

internal class RealAmountFormatterTest {
    private val numberFormatterFactory = RealNumberFormatterFactory()
    private val paymentAssetBrandProvider = FakePaymentAssetBrandProvider(PUBLISHED_SYMBOL)
    private val tokenAmountFormatter = RealTokenAmountFormatter(numberFormatterFactory, paymentAssetBrandProvider)

    @Test
    fun test() = with(TokenSymbolAppearance.Symbol("DOT")) {
        runFormattingTest("0.00000001 DOT", "0.000000011676979")
        runFormattingTest("0.00002 DOT", "0.000021676979")
        runFormattingTest("0.315 DOT", "0.315000041811")
        runFormattingTest("0.999 DOT", "0.99999999999")
        runFormattingTest("999.999 DOT", "999.99999999")
        runFormattingTest("1M DOT", "1000000")
        runFormattingTest("888,888.12 DOT", "888888.1234")
        runFormattingTest("1.24M DOT", "1243000")
        runFormattingTest("1.24M DOT", "1243011")
        runFormattingTest("100.04B DOT", "100041000000")
        runFormattingTest("1T DOT", "1001000000000")
        runFormattingTest("1,001T DOT", "1001000000000000")
    }

    @Test
    fun digitalDollarTest() = with(TokenSymbolAppearance.DigitalDollar) {
        runFormattingTest("0.50 CASH", "0.5")
        runFormattingTest("0.50 CASH", "0.50")
        runFormattingTest("0.80 CASH", "0.8")
        runFormattingTest("2.50 CASH", "2.5")
        runFormattingTest("2 CASH", "2")
        runFormattingTest("1,234.50 CASH", "1234.5")
        runFormattingTest("1,500 CASH", "1500")
        runFormattingTest("1.5M CASH", "1500000")
        runFormattingTest("2M CASH", "2000000")
        runFormattingTest("3.4B CASH", "3400000000")
        runFormattingTest("1.50M CASH", "1500000.5")
        runFormattingTest("2.23M CASH", "2234567.89")
    }

    @Test
    fun `should follow the published symbol for the digital dollar`() {
        paymentAssetBrandProvider.brand.value = PaymentAssetBrand.bundled("USD")

        with(TokenSymbolAppearance.DigitalDollar) {
            runFormattingTest("2 USD", "2")
        }
    }

    private fun TokenSymbolAppearance.runFormattingTest(
        expectedResult: String,
        unformattedAmountInput: String,
        precision: RoundPrecision = RoundPrecision.DEFAULT
    ) {
        val tokenAmountModel = object : TokenAmountModel {
            override val amount = unformattedAmountInput.toBigDecimal()
            override val appearance = this@runFormattingTest
        }
        val result = tokenAmountFormatter.formatTokenAmount(tokenAmountModel, precision)
        assertEquals(expectedResult, result)
    }

    private class FakePaymentAssetBrandProvider(symbol: String) : PaymentAssetBrandProvider {
        override val brand = MutableStateFlow(PaymentAssetBrand.bundled(symbol))
    }
}
