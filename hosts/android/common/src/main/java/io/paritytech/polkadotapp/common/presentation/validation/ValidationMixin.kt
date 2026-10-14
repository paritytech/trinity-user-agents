package io.paritytech.polkadotapp.common.presentation.validation

import io.paritytech.polkadotapp.common.domain.validation.Validation
import io.paritytech.polkadotapp.common.domain.validation.ValidationProcess
import io.paritytech.polkadotapp.common.domain.validation.ValidationResult
import io.paritytech.polkadotapp.common.domain.validation.ValidationUserInputAction
import io.paritytech.polkadotapp.common.utils.AwaitableActionChannel
import io.paritytech.polkadotapp.common.utils.AwaitableActionFlow
import io.paritytech.polkadotapp.common.utils.awaitAction
import kotlinx.coroutines.flow.receiveAsFlow
import javax.inject.Inject

interface ValidationMixin {
    companion object {
        fun create(): ValidationMixin = RealValidationMixin()
    }

    val userInputAction: AwaitableActionFlow<ValidationUserInputAction<*>, Any>

    suspend fun <P> runValidation(validation: Validation<P>, payload: P): ValidationResult<P>
}

internal class RealValidationMixin @Inject constructor() : ValidationMixin {
    private val _userInputAction = AwaitableActionChannel<ValidationUserInputAction<*>, Any>()
    override val userInputAction: AwaitableActionFlow<ValidationUserInputAction<*>, Any> = _userInputAction.receiveAsFlow()

    override suspend fun <P> runValidation(validation: Validation<P>, payload: P): ValidationResult<P> {
        return with(RealValidationProcess()) {
            validation.validate(payload)
        }
    }

    private inner class RealValidationProcess : ValidationProcess {
        @Suppress("UNCHECKED_CAST")
        override suspend fun <R> presentUserInput(action: ValidationUserInputAction<R>): R {
            return _userInputAction.awaitAction(action) as R
        }
    }
}
