package io.paritytech.polkadotapp.common.domain.validation.composite

import io.paritytech.polkadotapp.common.domain.validation.Validation

class CompositeValidationBuilder<P> internal constructor() {
    private val delegates = mutableListOf<Validation<P>>()

    fun add(validation: Validation<P>) {
        delegates.add(validation)
    }

    internal fun build(): CompositeValidation<P> {
        return CompositeValidation(delegates)
    }
}
