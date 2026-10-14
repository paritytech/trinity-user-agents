package io.paritytech.polkadotapp.common.domain.validation

interface ValidationProcess {
    suspend fun <R> presentUserInput(action: ValidationUserInputAction<R>): R
}

interface ValidationUserInputAction<RETURN>
