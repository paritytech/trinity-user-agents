package io.paritytech.polkadotapp.chains.network.binding

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.definitions.types.fromByteArrayOrNull
import io.novasama.substrate_sdk_android.runtime.metadata.module.Constant
import io.paritytech.polkadotapp.common.data.substrate.incompatible
import java.math.BigInteger

fun bindNumberConstant(
    constant: Constant,
    runtime: RuntimeSnapshot
): BigInteger = bindNullableNumberConstant(constant, runtime) ?: incompatible()

fun bindNullableNumberConstant(
    constant: Constant,
    runtime: RuntimeSnapshot
): BigInteger? {
    val decoded = constant.type?.fromByteArrayOrNull(runtime, constant.value) ?: incompatible()

    return decoded as BigInteger?
}
