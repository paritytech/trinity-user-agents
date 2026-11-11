package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.formatFiatSigned
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import java.math.BigDecimal
import java.math.RoundingMode
import javax.inject.Inject

/** Written in full, never "1.5M": the keypad takes seven whole digits and the screens echo what was typed. */
class FundingCashFormat @Inject constructor(
    private val formatter: TokenAmountFormatter,
    private val mapper: TokenAmountMapper,
) {
    fun figure(value: BigDecimal): String = formatter.formatFiatSigned(mapper.digitalDollar(value), abbreviate = false)

    fun label(value: BigDecimal): String =
        formatter.formatFiatSigned(mapper.digitalDollar(value), withSymbol = true, abbreviate = false)

    fun symbol(): String = formatter.formatToSymbol(mapper.digitalDollar(BigDecimal.ZERO))

    /** Rounded up, so a fee never reads as less than it is. */
    fun cost(value: BigDecimal): String = label(value.setScale(CENTS, RoundingMode.UP))

    private companion object {
        const val CENTS = 2
    }
}
