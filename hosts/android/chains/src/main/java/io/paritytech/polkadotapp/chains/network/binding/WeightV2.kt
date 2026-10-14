package io.paritytech.polkadotapp.chains.network.binding

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.Scale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.decode
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.encode
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import io.paritytech.polkadotapp.common.utils.Min
import io.paritytech.polkadotapp.common.utils.atLeastZero
import io.paritytech.polkadotapp.common.utils.scale.ToDynamicScaleInstance
import io.paritytech.polkadotapp.common.utils.times
import kotlinx.serialization.Serializable
import java.math.BigInteger

@Serializable
data class WeightV2(
    val refTime: BigIntegerSerializable,
    val proofSize: BigIntegerSerializable
) : Min<WeightV2>, ToDynamicScaleInstance {
    constructor(refTime: Int, proofSize: Int) : this(refTime.toBigInteger(), proofSize.toBigInteger())

    companion object {
        val MAX_DIMENSION = "184467440737090".toBigInteger()

        fun max(): WeightV2 {
            return WeightV2(MAX_DIMENSION, MAX_DIMENSION)
        }

        fun fromV1(refTime: BigInteger): WeightV2 {
            return WeightV2(refTime, proofSize = BigInteger.ZERO)
        }

        fun zero(): WeightV2 {
            return WeightV2(BigInteger.ZERO, BigInteger.ZERO)
        }

        fun bind(decoded: Any?): WeightV2 {
            return Scale.decode(decoded)
        }
    }

    operator fun times(multiplier: Double): WeightV2 {
        return WeightV2(refTime = refTime.times(multiplier), proofSize = proofSize.times(multiplier))
    }

    operator fun plus(other: WeightV2): WeightV2 {
        return WeightV2(refTime + other.refTime, proofSize + other.proofSize)
    }

    operator fun minus(other: WeightV2): WeightV2 {
        return WeightV2(
            refTime = (refTime - other.refTime).atLeastZero(),
            proofSize = (proofSize - other.proofSize).atLeastZero()
        )
    }

    override fun min(other: WeightV2): WeightV2 {
        return WeightV2(
            refTime = refTime.min(other.refTime),
            proofSize = proofSize.min(other.proofSize)
        )
    }

    override fun toEncodableInstance(): Any? {
        return Scale.encode(this)
    }
}

fun WeightV2.fitsIn(limit: WeightV2): Boolean {
    return refTime <= limit.refTime && proofSize <= limit.proofSize
}
