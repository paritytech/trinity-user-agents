package io.paritytech.polkadotapp.common.utils

import java.math.BigDecimal
import java.math.BigInteger
import java.math.MathContext
import java.nio.ByteBuffer
import java.nio.ByteOrder

fun BigInteger?.orZero(): BigInteger = this ?: BigInteger.ZERO

fun BigDecimal?.orZero(): BigDecimal = this ?: 0.toBigDecimal()

fun Int?.orZero(): Int = this ?: 0

fun Double?.orZero(): Double = this ?: 0.0

fun Double.ceil(): Double = kotlin.math.ceil(this)

fun BigInteger.atLeastZero() = coerceAtLeast(BigInteger.ZERO)

fun BigDecimal.atLeastZero() = coerceAtLeast(BigDecimal.ZERO)

fun BigDecimal.isZero(): Boolean {
    return signum() == 0
}

infix fun BigDecimal.hasTheSaveValueAs(another: BigDecimal) = compareTo(another) == 0

infix fun Long.ceilDiv(divisor: Long): Long = this / divisor + if (this % divisor == 0L) 0 else 1

inline fun <T> Collection<T>.sumByBigInteger(extractor: (T) -> BigInteger): BigInteger = fold(BigInteger.ZERO) { acc, element ->
    acc + extractor(element)
}

fun Iterable<BigInteger>.sum() = sumOf { it }

fun Int.toBigEndianByteArray(): ByteArray {
    return ByteBuffer.allocate(4).order(ByteOrder.BIG_ENDIAN).putInt(this).array()
}

fun ByteArray.toBigEndianInt(): Int {
    return ByteBuffer.wrap(this).order(ByteOrder.BIG_ENDIAN).int
}

fun Long.toLittleEndianBytes(): ByteArray =
    ByteBuffer.allocate(Long.SIZE_BYTES)
        .order(ByteOrder.LITTLE_ENDIAN)
        .putLong(this)
        .array()

fun UInt.toLittleEndianBytes(): ByteArray =
    ByteBuffer.allocate(UInt.SIZE_BYTES)
        .order(ByteOrder.LITTLE_ENDIAN)
        .putInt(toInt())
        .array()

fun BigInteger.divideToDecimal(divisor: BigInteger, mathContext: MathContext = MathContext.DECIMAL64): BigDecimal {
    return toBigDecimal().divide(divisor.toBigDecimal(), mathContext)
}

fun BigInteger.isZero(): Boolean {
    return signum() == 0
}

operator fun BigInteger.times(double: Double): BigInteger = toBigDecimal().multiply(double.toBigDecimal()).toBigInteger()
