package io.paritytech.polkadotapp.feature_coinage_api.domain.model

import kotlinx.serialization.Serializable
import java.math.BigDecimal
import java.math.MathContext

@JvmInline
@Serializable
value class ValueExponent(val value: Int) : Comparable<ValueExponent> {
    override fun compareTo(other: ValueExponent): Int {
        return value.compareTo(other.value)
    }
}

fun ValueExponent.tokenAmount(): BigDecimal {
    return BigDecimal.valueOf(2).pow(value, MathContext.DECIMAL128)
}
