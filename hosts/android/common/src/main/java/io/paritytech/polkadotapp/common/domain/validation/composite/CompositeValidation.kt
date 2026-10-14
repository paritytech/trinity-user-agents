package io.paritytech.polkadotapp.common.domain.validation.composite

import io.paritytech.polkadotapp.common.domain.validation.Validation
import io.paritytech.polkadotapp.common.domain.validation.ValidationProcess
import io.paritytech.polkadotapp.common.domain.validation.ValidationResult

class CompositeValidation<P> internal constructor(
    private val delegates: List<Validation<P>>
) : Validation<P> {
    context(validationProcess: ValidationProcess)
    override suspend fun validate(payload: P): ValidationResult<P> {
        return try {
            val initial: ValidationResult<P> = ValidationResult.Success(payload)

            return delegates.fold(initial) { acc, validation ->
                acc.fold(validation)
            }
        } catch (t: Throwable) {
            ValidationResult.Error(t)
        }
    }

    context(validationProcess: ValidationProcess)
    private suspend fun ValidationResult<P>.fold(validation: Validation<P>): ValidationResult<P> {
        return when (this) {
            ValidationResult.Aborted -> this

            is ValidationResult.Success -> validation.validate(finalPayload)

            is ValidationResult.Error -> this
        }
    }
}

fun <P> CompositeValidation(building: CompositeValidationBuilder<P>.() -> Unit): CompositeValidation<P> {
    return CompositeValidationBuilder<P>().apply(building).build()
}
