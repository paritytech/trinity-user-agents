package io.paritytech.polkadotapp.feature_products_api.presentation.deeplink

import android.net.Uri

/**
 * Whether the host opens product deeplinks at all. Off the arbitrary-products flag none open, so no
 * link can put an unvetted product on screen or run its worker.
 *
 * Shared by every dotNS handler: a product the app refuses to browse must not become reachable
 * through another target either.
 */
class ProductDeepLinkGate(
    private val arbitraryProductsEnabled: Boolean,
) {
    fun opens(): Boolean = arbitraryProductsEnabled
}
