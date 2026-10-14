package io.paritytech.polkadotapp.feature_products_api.model.signing

import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class SigningContextHolder @Inject constructor() {
    private var context: SigningContext? = null

    fun set(context: SigningContext) {
        this.context = context
    }

    fun get(): SigningContext? = context

    /**
     * Clears the holder only while it still belongs to [owner]. A signing
     * screen's ViewModel is cleared after its dismiss animation, by which time
     * the holder may already carry the next request's context; an
     * unconditional clear would wipe it and crash that screen on recreation.
     */
    fun clear(owner: SigningContext) {
        if (context === owner) {
            context = null
        }
    }
}
