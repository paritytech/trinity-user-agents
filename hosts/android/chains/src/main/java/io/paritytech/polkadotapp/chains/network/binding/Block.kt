package io.paritytech.polkadotapp.chains.network.binding

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import kotlinx.serialization.Serializable
import java.math.BigInteger

@JvmInline
@Serializable
value class BlockNumber(val value: BigIntegerSerializable) {
    companion object {
        val ZERO = BlockNumber(BigInteger.ZERO)
        val ONE = BlockNumber(BigInteger.ONE)
    }

    operator fun plus(other: BlockNumber): BlockNumber {
        return BlockNumber(value + other.value)
    }

    operator fun minus(other: BlockNumber): BlockNumber {
        return BlockNumber(value - other.value)
    }

    operator fun compareTo(other: BlockNumber): Int {
        return value.compareTo(other.value)
    }
}

typealias BlockHash = String

fun bindBlockNumber(dynamic: Any?) = bindNumber(dynamic).toBlockNumber()

fun BigInteger.toBlockNumber() = BlockNumber(this)
fun Int.toBlockNumber() = BlockNumber(this.toBigInteger())

fun Long.toBlockNumber() = BlockNumber(this.toBigInteger())
