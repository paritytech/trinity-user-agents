package io.paritytech.polkadotapp.feature_products_impl.domain.crossProductProof

import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class CrossProductProofContextHolder @Inject constructor() {
    private var context: CrossProductProofContext? = null

    fun set(context: CrossProductProofContext) {
        this.context = context
    }

    fun get(): CrossProductProofContext? = context

    fun clear() {
        context = null
    }
}
