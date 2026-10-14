package io.paritytech.polkadotapp.feature_transactions.api.data

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.metadata.error
import io.novasama.substrate_sdk_android.runtime.metadata.module
import io.novasama.substrate_sdk_android.runtime.metadata.module.ErrorMetadata
import io.paritytech.polkadotapp.chains.network.binding.bindInt
import io.paritytech.polkadotapp.common.data.substrate.castToDictEnum
import io.paritytech.polkadotapp.common.data.substrate.castToStruct
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module as RuntimeModule

sealed class DispatchError : Throwable() {
    data class Module(val module: RuntimeModule, val error: ErrorMetadata) : DispatchError() {
        override val message: String
            get() = toString()

        override fun toString(): String {
            return "${module.name}.${error.name}"
        }
    }

    data class Unknown(val data: String) : DispatchError()
}

fun bindDispatchError(decoded: Any?, runtimeSnapshot: RuntimeSnapshot): DispatchError {
    val asDictEnum = decoded.castToDictEnum()

    return when (asDictEnum.name) {
        "Module" -> {
            val moduleErrorStruct = asDictEnum.value.castToStruct()

            val moduleIndex = bindInt(moduleErrorStruct["index"])
            val errorIndex = bindModuleError(moduleErrorStruct["error"])

            val module = runtimeSnapshot.metadata.module(moduleIndex)
            val error = module.error(errorIndex)

            DispatchError.Module(module, error)
        }

        else -> DispatchError.Unknown(decoded.name)
    }
}

private fun bindModuleError(errorEncoded: ByteArray?): Int {
    requireNotNull(errorEncoded) {
        "Error should exist"
    }

    return errorEncoded[0].toInt()
}
