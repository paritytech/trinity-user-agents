package io.paritytech.polkadotapp.feature_prices_api.domain.model

import io.paritytech.polkadotapp.common.utils.orZero
import java.math.BigDecimal
import java.math.MathContext

class Price(
    val perUnitPrice: BigDecimal?,
    val currency: Currency,
) {
    companion object {
        fun empty(currency: Currency): Price {
            return Price(perUnitPrice = null, currency)
        }
    }
}

data class FiatAmount(
    val amountPrice: BigDecimal,
    val currency: Currency
)

operator fun FiatAmount.plus(other: FiatAmount): FiatAmount {
    require(currency.id == other.currency.id) {
        "Cannot add priced amounts with different currencies"
    }

    return this plusInternal other
}

infix fun FiatAmount.plusOrNull(other: FiatAmount): FiatAmount? {
    if (currency.id != other.currency.id) {
        return null
    }

    return this plusInternal other
}

inline fun <T> List<T>.sumOfOrThrow(extractFiat: (T) -> FiatAmount): FiatAmount {
    return map(extractFiat)
        .reduce { acc, amount -> acc.plus(amount) }
}

private infix fun FiatAmount.plusInternal(other: FiatAmount): FiatAmount {
    return FiatAmount(amountPrice = amountPrice + other.amountPrice, currency = other.currency)
}

fun Price.priceOf(amount: BigDecimal): FiatAmount {
    return FiatAmount(
        amountPrice = perUnitPrice.orZero() * amount,
        currency = currency
    )
}

fun Price.amountOf(price: BigDecimal): BigDecimal {
    val perUnit = perUnitPrice.orZero()

    if (perUnit == BigDecimal.ZERO) return BigDecimal.ZERO

    return price.divide(perUnit, MathContext.DECIMAL128)
}
