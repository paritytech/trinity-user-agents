package io.paritytech.polkadotapp.chains.network.binding

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import io.paritytech.polkadotapp.common.utils.divideToDecimal
import kotlinx.serialization.Serializable
import java.math.BigDecimal
import java.math.BigInteger

@JvmInline
@Serializable
value class Balance(val value: BigIntegerSerializable) : Comparable<Balance> {
    companion object {
        val ZERO = Balance(BigInteger.ZERO)

        val ONE = Balance(BigInteger.ONE)
    }

    operator fun plus(other: Balance): Balance {
        return Balance(value + other.value)
    }

    operator fun minus(other: Balance): Balance {
        return Balance(value - other.value)
    }

    override operator fun compareTo(other: Balance): Int {
        return this.value.compareTo(other.value)
    }

    operator fun div(other: Balance): BigDecimal {
        return value.divideToDecimal(other.value)
    }

    operator fun div(other: Int): Balance {
        return Balance(value / other.toBigInteger())
    }

    operator fun times(other: BigDecimal): Balance {
        return value.toBigDecimal().multiply(other).toBigInteger().intoBalance()
    }

    operator fun times(other: Int): Balance {
        return value.multiply(other.toBigInteger()).intoBalance()
    }

    operator fun times(other: Double): Balance {
        return times(other.toBigDecimal())
    }

    fun signum(): Int {
        return value.signum()
    }

    fun atLeastZero(): Balance = Balance(value.coerceAtLeast(BigInteger.ZERO))

    fun isPositive(): Boolean = value.signum() == 1
    fun iNegative(): Boolean = value.signum() == -1

    fun isZero(): Boolean = value.signum() == 0
}

fun bindBalance(dynamic: Any?) = bindNumber(dynamic).intoBalance()

fun BigInteger.intoBalance() = Balance(this)
fun Int.intoBalance() = toBigInteger().intoBalance()
fun Long.intoBalance() = toBigInteger().intoBalance()

fun Balance?.orZero() = this ?: Balance.ZERO
fun Balance.toInt() = value.toInt()
fun max(a: Balance, b: Balance): Balance = if (a >= b) a else b

inline fun <T> Collection<T>.sumByBalance(extractor: (T) -> Balance): Balance = fold(Balance.ZERO) { acc, element ->
    acc + extractor(element)
}
