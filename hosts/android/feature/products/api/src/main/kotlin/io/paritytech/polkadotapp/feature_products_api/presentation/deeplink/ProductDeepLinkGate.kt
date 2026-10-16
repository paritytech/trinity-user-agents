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

/** The first path segment the core reserves for targets the host answers itself, rather than the product. */
private const val RESERVED_SEGMENT = "-"
private const val POCKET_SEGMENT = "pocket"

/** `/-/pocket/…`, the one reserved target the host claims so far. */
fun Uri.isPocketTarget(): Boolean = pathSegments.take(2) == listOf(RESERVED_SEGMENT, POCKET_SEGMENT)
