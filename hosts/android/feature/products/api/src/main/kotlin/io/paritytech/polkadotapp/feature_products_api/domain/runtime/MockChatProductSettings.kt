package io.paritytech.polkadotapp.feature_products_api.domain.runtime

import kotlinx.coroutines.flow.Flow

/**
 * Whether the chat list shows the chat showcase: a product that the debug build type serves from the app itself, so
 * a tester sees everything that a product can draw in a room without a worker or a network.
 */
interface MockChatProductSettings {
    /** The current choice, then each change. Other build types have no showcase, so the choice does nothing there. */
    fun observeEnabled(): Flow<Boolean>

    fun setEnabled(enabled: Boolean)
}
