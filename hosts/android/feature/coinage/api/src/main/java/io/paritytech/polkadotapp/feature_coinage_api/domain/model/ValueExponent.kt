package io.paritytech.polkadotapp.feature_coinage_api.domain.model

import io.paritytech.polkadotapp.common.utils.toLittleEndianBytes
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
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

private val RECYCLER_PREFIX = "coinage/recycler".toByteArray()

fun ValueExponent.toRingCollectionId(instanceId: CoinageInstanceId): RingCollectionId {
    val prefix = RECYCLER_PREFIX + instanceId.toLittleEndianBytes() + value.toByte()
    return RingCollectionId.paddedBytes(prefix)
}
