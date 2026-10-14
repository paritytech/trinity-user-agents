package io.paritytech.polkadotapp.common.utils

import kotlin.contracts.ExperimentalContracts
import kotlin.contracts.InvocationKind
import kotlin.contracts.contract

@OptIn(ExperimentalContracts::class)
@JvmInline
value class InformationSize(private val sizeInBytes: Long) : Comparable<InformationSize> {
    companion object {
        val ZERO = InformationSize(0)

        val Int.bytes: InformationSize get() = toInformationSize(InformationSizeUnit.BYTES)

        val Long.bytes: InformationSize get() = toInformationSize(InformationSizeUnit.BYTES)

        val Int.kilobytes: InformationSize get() = toInformationSize(InformationSizeUnit.KILOBYTES)

        val Long.kilobytes: InformationSize get() = toInformationSize(InformationSizeUnit.KILOBYTES)

        val Int.megabytes: InformationSize get() = toInformationSize(InformationSizeUnit.MEGABYTES)

        val Long.megabytes: InformationSize get() = toInformationSize(InformationSizeUnit.MEGABYTES)

        val Int.gigabytes: InformationSize get() = toInformationSize(InformationSizeUnit.GIGABYTES)
    }

    val inWholeBytes: Long get() = toWholeUnits(InformationSizeUnit.BYTES)

    val inWholeKilobytes: Long get() = toWholeUnits(InformationSizeUnit.KILOBYTES)

    val inWholeMegabytes: Long get() = toWholeUnits(InformationSizeUnit.MEGABYTES)

    val inWholeGigabytes: Long get() = toWholeUnits(InformationSizeUnit.GIGABYTES)

    val bytesComponent: Long get() = inWholeBytes % 1024

    val kilobytesComponent: Long get() = inWholeKilobytes % 1024

    val megabytesComponent: Long get() = inWholeMegabytes % 1024

    inline fun <T> toComponents(action: (gigabytes: Long, megabytes: Long, kilobytes: Long, bytes: Long) -> T): T {
        contract { callsInPlace(action, InvocationKind.EXACTLY_ONCE) }
        return action(inWholeGigabytes, megabytesComponent, kilobytesComponent, bytesComponent)
    }

    override fun toString(): String {
        return toComponents { gigabytes, megabytes, kilobytes, bytes ->
            when {
                gigabytes > 0 -> gigabytes.toString() + "GB"
                megabytes > 0 -> megabytes.toString() + "MB"
                kilobytes > 0 -> kilobytes.toString() + "KB"
                else -> bytes.toString() + "B"
            }
        }
    }

    fun toWholeUnits(unit: InformationSizeUnit): Long {
        return unit.convertFromBytes(sizeInBytes)
    }

    override fun compareTo(other: InformationSize): Int {
        return sizeInBytes.compareTo(other.sizeInBytes)
    }

    operator fun plus(other: InformationSize): InformationSize {
        return InformationSize(sizeInBytes + other.sizeInBytes)
    }

    operator fun minus(other: InformationSize): InformationSize {
        return InformationSize(sizeInBytes - other.sizeInBytes)
    }
}

enum class InformationSizeUnit {
    BYTES,

    KILOBYTES,

    MEGABYTES,

    GIGABYTES
}

fun Int.toInformationSize(unit: InformationSizeUnit): InformationSize {
    return toLong().toInformationSize(unit)
}

fun Long.toInformationSize(unit: InformationSizeUnit): InformationSize {
    return InformationSize(unit.convertToBytes(this))
}

private fun InformationSizeUnit.convertToBytes(value: Long): Long {
    return when (this) {
        InformationSizeUnit.BYTES -> value
        InformationSizeUnit.KILOBYTES -> value * 1024
        InformationSizeUnit.MEGABYTES -> value * 1024 * 1024
        InformationSizeUnit.GIGABYTES -> value * 1024 * 1024 * 1024
    }
}

private fun InformationSizeUnit.convertFromBytes(value: Long): Long {
    return when (this) {
        InformationSizeUnit.BYTES -> value
        InformationSizeUnit.KILOBYTES -> value / 1024
        InformationSizeUnit.MEGABYTES -> value / 1024 / 1024
        InformationSizeUnit.GIGABYTES -> value / 1024 / 1024 / 1024
    }
}
