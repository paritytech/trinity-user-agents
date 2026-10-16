package io.paritytech.polkadotapp.feature_tokens_impl.presentation.formatter

import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatterFactory
import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatterFactory.Params
import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatterFactory.Params.KnownAbbreviation
import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatterFactory.Params.SmallNumberParams
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrandProvider
import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenSymbolAppearance
import java.math.BigDecimal
import java.math.RoundingMode
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealTokenAmountFormatter @Inject constructor(
    private val numberFormatterFactory: NumberFormatterFactory,
    private val paymentAssetBrandProvider: PaymentAssetBrandProvider,
) : TokenAmountFormatter {
    private val tokenAmountFormatterFiat = createTokenAmountFormatter(RoundPrecision.FIAT)

    private val tokenAmountFormatterDefault = createTokenAmountFormatter(RoundPrecision.DEFAULT)

    private val tokenAmountFormatterHigh = createTokenAmountFormatter(RoundPrecision.HIGH)

    private val tokenAmountFormatterDigitalDollar = createTokenAmountFormatter(RoundPrecision.FIAT, padTrailingZeros = true)

    private fun createTokenAmountFormatter(roundPrecision: RoundPrecision, padTrailingZeros: Boolean = false) =
        numberFormatterFactory.createFormatter(
            params = Params(
                includedAbbreviations = KnownAbbreviation.all(),
                unabbreviatedTotalDecimals = roundPrecision.digits,
                unabbreviatedMinTotalDecimals = roundPrecision.minDigits,
                abbreviatedTotalDecimals = 2,
                abbreviatedMinTotalDecimals = if (padTrailingZeros) 2 else 0,
                smallNumberParams = SmallNumberParams(
                    minSignificantDecimals = 1,
                    minTotalDecimals = roundPrecision.digits,
                    padTrailingZeros = padTrailingZeros
                ),
                roundingMode = RoundingMode.DOWN
            )
        )

    private val percentFormatter = numberFormatterFactory.createFormatter(
        params = Params(
            includedAbbreviations = emptyList(),
            unabbreviatedTotalDecimals = 2,
            unabbreviatedMinTotalDecimals = 2,
            abbreviatedTotalDecimals = 2,
            abbreviatedMinTotalDecimals = 0,
            smallNumberParams = SmallNumberParams(
                minSignificantDecimals = 1,
                minTotalDecimals = 2,
                padTrailingZeros = false
            ),
            roundingMode = RoundingMode.DOWN
        )
    )

    override fun formatTokenAmount(
        tokenAmount: TokenAmountModel,
        precision: RoundPrecision,
        approx: Boolean,
        withSymbol: Boolean
    ): String {
        val formatter = precision.formatter()
        return buildString {
            val number = when (tokenAmount.appearance) {
                TokenSymbolAppearance.DigitalDollar -> tokenAmount.amount.formatAsDigitalDollar()

                is TokenSymbolAppearance.Symbol -> formatter.format(tokenAmount.amount)
            }

            if (approx) append("≈ ")
            append(number)
            if (withSymbol) append(" ${formatToSymbol(tokenAmount)}")
        }
    }

    private fun BigDecimal.formatAsDigitalDollar(): String {
        val isWhole = stripTrailingZeros().scale() <= 0
        return if (isWhole) {
            tokenAmountFormatterDefault.format(this)
        } else {
            tokenAmountFormatterDigitalDollar.format(this)
        }
    }

    override fun formatToSymbol(tokenAmount: TokenAmountModel): String = when (val appearance = tokenAmount.appearance) {
        TokenSymbolAppearance.DigitalDollar -> paymentAssetBrandProvider.brand.value.symbol
        is TokenSymbolAppearance.Symbol -> appearance.symbol
    }

    override fun formatAmount(amount: BigDecimal, precision: RoundPrecision): String {
        return precision.formatter().format(amount)
    }

    override fun formatPercent(value: Fraction): String {
        val toFormat = value.inPercents
        return formatPercent(toFormat)
    }

    override fun formatIntegralPercent(value: Fraction): String {
        val roundedPercents = value.inPercents.setScale(0, RoundingMode.DOWN)
        return formatPercent(roundedPercents)
    }

    private fun formatPercent(value: BigDecimal): String {
        return "${percentFormatter.format(value)}%"
    }

    private fun RoundPrecision.formatter() = when (this) {
        RoundPrecision.FIAT -> tokenAmountFormatterFiat
        RoundPrecision.DEFAULT -> tokenAmountFormatterDefault
        RoundPrecision.HIGH -> tokenAmountFormatterHigh
    }
}
