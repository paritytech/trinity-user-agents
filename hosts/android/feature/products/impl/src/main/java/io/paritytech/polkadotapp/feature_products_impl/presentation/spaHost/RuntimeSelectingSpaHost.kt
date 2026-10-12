package io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.screens.MessageDisplay
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHost
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHostSession
import javax.inject.Inject

/**
 * Picks the host runtime per session: the mode is read at [createSession], so
 * flipping the debug toggle affects the next session, never a live one.
 */
class RuntimeSelectingSpaHost @Inject constructor(
    private val native: NativeSpaHost,
    private val truapi: TrUAPISpaHost,
    private val runtimeSettings: ProductRuntimeSettings,
) : SpaHost {
    context(scope: ComputationalScope, messageDisplay: MessageDisplay)
    override fun createSession(initialUrl: String, underCard: Boolean): SpaHostSession =
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) {
            truapi.createSession(initialUrl, underCard)
        } else {
            native.createSession(initialUrl)
        }
}
