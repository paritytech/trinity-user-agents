package io.paritytech.polkadotapp.chains.network.binding

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import io.paritytech.polkadotapp.common.data.substrate.castToDictEnum
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
sealed class ScaleResult<out T, out E> {
    @Serializable
    @SerialName("Ok")
    @TransientStruct
    class Ok<T>(val value: T) : ScaleResult<T, Nothing>()

    @Serializable
    @SerialName("Err")
    @TransientStruct
    class Error<E>(val error: E) : ScaleResult<Nothing, E>()

    companion object {
        fun <T, E> bind(
            dynamicInstance: Any?,
            bindOk: (Any?) -> T,
            bindError: (Any?) -> E
        ): ScaleResult<T, E> {
            val asEnum = dynamicInstance.castToDictEnum()

            return when (asEnum.name) {
                "Ok" -> Ok(bindOk(asEnum.value))
                "Err" -> Error(bindError(asEnum.value))
                else -> error("Unknown Result variant: ${asEnum.name}")
            }
        }
    }
}

class ScaleResultError(val content: Any?) : Throwable() {
    override val message: String?
        get() = content.toString()
}

inline fun <T, E, R> ScaleResult<T, E>.mapError(transform: (E) -> R): ScaleResult<T, R> {
    return when (this) {
        is ScaleResult.Ok -> this
        is ScaleResult.Error -> ScaleResult.Error(transform(error))
    }
}

fun <T, R> ScaleResult<T, R>.toResult(): Result<T> {
    return when (this) {
        is ScaleResult.Error -> Result.failure(ScaleResultError(error))
        is ScaleResult.Ok -> Result.success(value)
    }
}
