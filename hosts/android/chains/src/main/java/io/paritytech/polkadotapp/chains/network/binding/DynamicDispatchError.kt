package io.paritytech.polkadotapp.chains.network.binding

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.SerializedFallback
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.ByteArraySerializable
import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.metadata.error
import io.novasama.substrate_sdk_android.runtime.metadata.module
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Pure dynamic SCALE model for Substrate `DispatchError`.
 *
 * Captures the raw deserialized structure without requiring runtime context.
 * Use [toDispatchError] to resolve into a domain [DispatchError] when runtime is available.
 */
@Serializable
@SerializedFallback("Unknown")
sealed class DynamicDispatchError {
    @Serializable
    @SerialName("Module")
    @TransientStruct
    data class Module(val value: ModuleError) : DynamicDispatchError()

    @Serializable
    @SerialName("Token")
    data object Token : DynamicDispatchError()

    @Serializable
    data object Unknown : DynamicDispatchError()
}

@Serializable
class ModuleError(
    val index: BigIntegerSerializable,
    val error: ByteArraySerializable
)

fun DynamicDispatchError.toDispatchError(runtime: RuntimeSnapshot): DispatchError {
    return when (this) {
        is DynamicDispatchError.Module -> {
            val moduleIndex = value.index.toInt()
            val errorIndex = value.error[0].toInt()

            val module = runtime.metadata.module(moduleIndex)
            val error = module.error(errorIndex)

            DispatchError.Module(module, error)
        }

        is DynamicDispatchError.Token -> DispatchError.Token
        is DynamicDispatchError.Unknown -> DispatchError.Unknown
    }
}
