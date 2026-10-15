package io.paritytech.polkadotapp.feature_products_api.presentation.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.feature_products_api.domain.FundingDomainProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

/**
 * Which products the host opens deeplinks for. Off the arbitrary-products flag only the app's own
 * funding products are reachable, so no link can put an unvetted product on screen or run its worker.
 *
 * Shared by every dotNS handler: a product the app refuses to browse must not become reachable
 * through another target either.
 */
class ProductDeepLinkGate(
    private val arbitraryProductsEnabled: Boolean,
    private val fundingDomainProvider: FundingDomainProvider,
) {
    suspend fun opens(productId: ProductId?): Boolean {
        if (arbitraryProductsEnabled) return true
        if (productId == null) return false

        return productId in fundingDomainProvider.getFundingProductIds().getOrNull().orEmpty()
    }
}

/** The first path segment the core reserves for targets the host answers itself, rather than the product. */
private const val RESERVED_SEGMENT = "-"
private const val POCKET_SEGMENT = "pocket"

private const val CHAT_SEGMENT = "chat"
private const val CHAT_ROOM_PATH_SIZE = 3

/** `/-/pocket/…`, the Pocket target that the host claims. */
fun Uri.isPocketTarget(): Boolean = pathSegments.take(2) == listOf(RESERVED_SEGMENT, POCKET_SEGMENT)

/** `/-/chat/<room>`, the link into a chat room of the product (02-DESIGN §13 item 15). */
fun Uri.isChatRoomTarget(): Boolean = chatRoomIdOf(pathSegments) != null

/** A target under the reserved segment that the host answers itself, not the App of the product. */
fun Uri.isHostTarget(): Boolean = isPocketTarget() || isChatRoomTarget()

/** The room of `/-/chat/<room>` path segments, decoded and trimmed, or null for another path. */
fun chatRoomIdOf(pathSegments: List<String>): String? {
    if (pathSegments.size != CHAT_ROOM_PATH_SIZE || pathSegments[0] != RESERVED_SEGMENT || pathSegments[1] != CHAT_SEGMENT) return null
    return pathSegments[2].trim().takeIf { it.isNotEmpty() }
}
