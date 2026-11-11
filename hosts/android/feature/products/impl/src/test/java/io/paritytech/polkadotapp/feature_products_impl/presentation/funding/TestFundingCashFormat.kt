package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetWithAmount
import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenSymbolAppearance
import java.math.BigDecimal
import java.math.RoundingMode
import java.text.DecimalFormat
import java.text.DecimalFormatSymbols
import java.util.Locale

/** The app formatter's CASH rules, in full: "$1,000.25 CASH", cents only when there are any, rounded down. */
fun testCashFormat(): FundingCashFormat = FundingCashFormat(CashOnlyFormatter, CashOnlyMapper)

private object CashOnlyFormatter : TokenAmountFormatter {
    override fun formatTokenAmount(
        tokenAmount: TokenAmountModel,
        precision: RoundPrecision,
        approx: Boolean,
        withSymbol: Boolean,
        abbreviate: Boolean,
    ): String {
        val value = tokenAmount.amount
        val format = DecimalFormat("#,##0", DecimalFormatSymbols(Locale.US)).apply {
            minimumFractionDigits = if (value.stripTrailingZeros().scale() > 0) 2 else 0
            maximumFractionDigits = 2
            roundingMode = RoundingMode.DOWN
        }
        return format.format(value) + if (withSymbol) " ${formatToSymbol(tokenAmount)}" else ""
    }

    override fun formatAmount(amount: BigDecimal, precision: RoundPrecision): String = amount.toPlainString()

    override fun formatToSymbol(tokenAmount: TokenAmountModel): String = "CASH"

    override fun formatPercent(value: Fraction): String = value.toString()

    override fun formatIntegralPercent(value: Fraction): String = value.toString()
}

private object CashOnlyMapper : TokenAmountMapper {
    override fun mapFrom(source: ChainAssetWithAmount): TokenAmountModel = error("CASH only")

    override fun digitalDollar(amount: BigDecimal): TokenAmountModel = object : TokenAmountModel {
        override val amount = amount
        override val appearance = TokenSymbolAppearance.DigitalDollar
    }
}
