package io.paritytech.polkadotapp.common.domain.validation

interface Validation<P> {
    context(validationProcess: ValidationProcess)
    suspend fun validate(payload: P): ValidationResult<P>
}
