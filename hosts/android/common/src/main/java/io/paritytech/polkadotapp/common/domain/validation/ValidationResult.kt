package io.paritytech.polkadotapp.common.domain.validation

sealed class ValidationResult<out P> {
    class Success<P>(val finalPayload: P) : ValidationResult<P>()

    data object Aborted : ValidationResult<Nothing>()

    data class Error(val error: Throwable) : ValidationResult<Nothing>()
}

fun ValidationResult<*>.onError(action: (error: Throwable) -> Unit) {
    if (this is ValidationResult.Error) action(error)
}

inline fun <P> ValidationResult<P>.onSuccess(action: (finalPayload: P) -> Unit) {
    if (this is ValidationResult.Success) action(finalPayload)
}

inline fun <T, P> Result<T>.toValidationResult(onSuccess: (T) -> ValidationResult<P>): ValidationResult<P> {
    return fold(
        onSuccess = onSuccess,
        onFailure = { ValidationResult.Error(it) }
    )
}
