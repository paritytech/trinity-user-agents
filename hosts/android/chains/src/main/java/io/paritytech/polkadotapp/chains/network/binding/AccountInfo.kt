package io.paritytech.polkadotapp.chains.network.binding

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import io.paritytech.polkadotapp.common.utils.isZero
import kotlinx.serialization.Serializable
import java.math.BigInteger

@Serializable
class AccountData(
    val free: Balance,
    val reserved: Balance,
    val frozen: Balance,
    val flags: AccountDataFlags,
)

@JvmInline
@Serializable
value class AccountDataFlags(val value: BigIntegerSerializable) {
    companion object {
        fun default() = AccountDataFlags(BigInteger.ZERO)

        private val HOLD_AND_FREEZES_ENABLED_MASK: BigInteger = BigInteger("80000000000000000000000000000000", 16)
    }

    fun holdsAndFreezesEnabled(): Boolean {
        return flagEnabled(HOLD_AND_FREEZES_ENABLED_MASK)
    }

    @Suppress("SameParameterValue")
    private fun flagEnabled(flag: BigInteger) = value and flag == flag
}

@Serializable
class AccountInfo(
    val consumers: BigIntegerSerializable,
    val providers: BigIntegerSerializable,
    val sufficients: BigIntegerSerializable,
    val data: AccountData,
) {
    companion object {
        fun empty() = AccountInfo(
            consumers = BigInteger.ZERO,
            providers = BigInteger.ZERO,
            sufficients = BigInteger.ZERO,
            data = AccountData(
                free = Balance.ZERO,
                reserved = Balance.ZERO,
                frozen = Balance.ZERO,
                flags = AccountDataFlags.default(),
            )
        )
    }
}

fun AccountInfo.canDecrementProvider(): Boolean {
    return providers > BigInteger.ONE || consumers.isZero()
}

fun AccountInfo?.orEmpty() = this ?: AccountInfo.empty()
