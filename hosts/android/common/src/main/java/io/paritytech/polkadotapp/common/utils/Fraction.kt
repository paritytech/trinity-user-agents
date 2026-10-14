package io.paritytech.polkadotapp.common.utils

import java.math.BigDecimal
import java.math.BigInteger

@JvmInline
value class Fraction private constructor(private val value: BigDecimal) : Comparable<Fraction> {
    companion object {
        val ZERO = Fraction(BigDecimal.ZERO)

        val FULL = Fraction(BigDecimal.ONE)

        fun BigDecimal.toFraction(unit: FractionUnit): Fraction {
            return Fraction(unit.convertToFraction(this))
        }

        fun BigInteger.toFraction(unit: FractionUnit): Fraction {
            return Fraction(unit.convertToFraction(toBigDecimal()))
        }

        fun Int.toFraction(unit: FractionUnit): Fraction {
            return Fraction(unit.convertToFraction(toBigDecimal()))
        }

        fun Double.toFraction(unit: FractionUnit): Fraction {
            return Fraction(unit.convertToFraction(toBigDecimal()))
        }

        val Int.percents: Fraction
            get() = toFraction(FractionUnit.PERCENT)

        val Double.percents: Fraction
            get() = toFraction(FractionUnit.PERCENT)

        val BigDecimal.asFraction: Fraction
            get() = toFraction(FractionUnit.FRACTION)
    }

    val inPercents: BigDecimal
        get() = FractionUnit.PERCENT.convertFromFraction(value)

    val fraction: BigDecimal
        get() = value

    override fun compareTo(other: Fraction): Int {
        return value.compareTo(other.value)
    }
}

enum class FractionUnit(val scale: Int) {
    /**
     * Divisor: 10^0
     */
    FRACTION(0),

    /**
     * Divisor: 10^2
     */
    PERCENT(2),

    /**
     * Divisor: 10^6
     */
    PERMILL(6),

    /**
     * Divisor: 10^18
     */
    PERQUINTILL(18)
}

private fun FractionUnit.convertToFraction(value: BigDecimal): BigDecimal {
    return value.scaleByPowerOfTen(-scale)
}

private fun FractionUnit.convertFromFraction(value: BigDecimal): BigDecimal {
    return value.scaleByPowerOfTen(scale)
}
