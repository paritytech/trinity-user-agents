package io.paritytech.polkadotapp.common.presentation.formatters.number

import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatterFactory.Params
import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatterFactory.Params.KnownAbbreviation
import io.paritytech.polkadotapp.common.presentation.formatters.number.internal.CompoundNumberFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.number.internal.CompoundNumberFormatter.NumberAbbreviation
import io.paritytech.polkadotapp.common.presentation.formatters.number.internal.DynamicPrecisionFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.number.internal.FixedPrecisionFormatter
import java.math.BigDecimal
import java.math.RoundingMode
import javax.inject.Inject
import javax.inject.Singleton

interface NumberFormatterFactory {
    class Params(
        /**
         * Which abbreviations to use while formatting
         */
        val includedAbbreviations: List<KnownAbbreviation>,
        /**
         * Decimal places when abbreviations are not used and the formatting number is greater or equal to 1
         */
        val unabbreviatedTotalDecimals: Int,
        /**
         * Minimum decimal places to always render for unabbreviated numbers >= 1, padded with trailing zeros.
         * Must be in 0..unabbreviatedTotalDecimals.
         */
        val unabbreviatedMinTotalDecimals: Int,
        /**
         * Decimal places for the mantissa of abbreviated numbers (e.g. the "1.24" in "1.24M").
         */
        val abbreviatedTotalDecimals: Int,
        /**
         * Minimum decimal places to always render for abbreviated numbers, padded with trailing zeros
         * (e.g. 0 -> "1.5M", 2 -> "1.50M"). Must be in 0..abbreviatedTotalDecimals.
         */
        val abbreviatedMinTotalDecimals: Int,
        /**
         * Configuration for formatting numbers smaller then 1
         */
        val smallNumberParams: SmallNumberParams,
        /**
         * Which rounding mode to use before formatting numbers
         */
        val roundingMode: RoundingMode,
    ) {
        enum class KnownAbbreviation {
            THOUSAND,
            MILLION,
            BILLION,
            TRILLION;

            companion object {
                fun all(): List<KnownAbbreviation> = entries
            }
        }

        class SmallNumberParams(
            /**
             * Minimum desired number of significant decimal places
             * Significant decimal places are places that go after initial zeros:
             *
             * Example:
             * 0.000**12** have 2 significant decimal places
             */
            val minSignificantDecimals: Int,
            /**
             * Minimum desired number of total decimals
             *
             * Example:
             * 0.**00012** have 5 decimals in total
             */
            val minTotalDecimals: Int,
            /**
             * When true, trailing zeros are kept up to [minTotalDecimals] (e.g. 0.5 -> 0.50).
             * When false, trailing zeros are trimmed (0.5 -> 0.5).
             */
            val padTrailingZeros: Boolean
        )
    }

    fun createFormatter(params: Params): NumberFormatter
}

@Singleton
class RealNumberFormatterFactory @Inject constructor() : NumberFormatterFactory {
    override fun createFormatter(params: Params): NumberFormatter {
        val abbreviations =
            buildList {
                add(params.createSmallNumberAbbreviation())
                add(params.createRegularNumberAbbreviation())
                addAll(params.createBigNumberAbbreviations())
            }

        return CompoundNumberFormatter(abbreviations)
    }

    private fun Params.createSmallNumberAbbreviation(): NumberAbbreviation {
        return NumberAbbreviation(
            threshold = BigDecimal.ZERO,
            divisor = BigDecimal.ONE,
            suffix = "",
            formatter = DynamicPrecisionFormatter(
                minScale = smallNumberParams.minTotalDecimals,
                minPrecision = smallNumberParams.minSignificantDecimals,
                padTrailingZeros = smallNumberParams.padTrailingZeros,
            ),
        )
    }

    private fun Params.createRegularNumberAbbreviation(): NumberAbbreviation {
        return NumberAbbreviation(
            threshold = BigDecimal.ONE,
            divisor = BigDecimal.ONE,
            suffix = "",
            formatter = FixedPrecisionFormatter(
                precision = unabbreviatedTotalDecimals,
                minPrecision = unabbreviatedMinTotalDecimals,
            ),
        )
    }

    private fun Params.createBigNumberAbbreviations(): List<NumberAbbreviation> {
        return includedAbbreviations.map { createAbbreviation(it) }
    }

    private fun Params.createAbbreviation(abbreviation: KnownAbbreviation): NumberAbbreviation {
        val defaultAbbreviationFormatter = FixedPrecisionFormatter(
            precision = abbreviatedTotalDecimals,
            minPrecision = abbreviatedMinTotalDecimals,
        )

        return when (abbreviation) {
            KnownAbbreviation.THOUSAND -> NumberAbbreviation(
                threshold = BigDecimal("1E+3"),
                divisor = BigDecimal.ONE,
                suffix = "",
                formatter = defaultAbbreviationFormatter,
            )

            KnownAbbreviation.MILLION -> NumberAbbreviation(
                threshold = BigDecimal("1E+6"),
                divisor = BigDecimal("1E+6"),
                suffix = "M",
                formatter = defaultAbbreviationFormatter,
            )

            KnownAbbreviation.BILLION -> NumberAbbreviation(
                threshold = BigDecimal("1E+9"),
                divisor = BigDecimal("1E+9"),
                suffix = "B",
                formatter = defaultAbbreviationFormatter,
            )

            KnownAbbreviation.TRILLION -> NumberAbbreviation(
                threshold = BigDecimal("1E+12"),
                divisor = BigDecimal("1E+12"),
                suffix = "T",
                formatter = defaultAbbreviationFormatter,
            )
        }
    }
}
