package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import uniffi.truapi.U128
import java.math.BigDecimal
import java.math.BigInteger
import java.math.RoundingMode

/** The payment balance's units, which cross the FFI as decimal strings of the asset's smallest unit. */
data class FundingCash(
    val precision: Int,
) {
    fun decimal(units: U128): BigDecimal =
        units.toBigIntegerOrNull()?.toBigDecimal()?.movePointLeft(precision)?.canonical() ?: BigDecimal.ZERO

    /** Rounds down, so the host never asks for more than the user typed. */
    fun units(value: BigDecimal): U128 =
        value.movePointRight(precision).setScale(0, RoundingMode.DOWN).toBigInteger().max(BigInteger.ZERO).toString()
}

/** One scale per value, so equal amounts compare equal as data. */
fun BigDecimal.canonical(): BigDecimal = stripTrailingZeros().let { if (it.scale() < 0) it.setScale(0) else it }
