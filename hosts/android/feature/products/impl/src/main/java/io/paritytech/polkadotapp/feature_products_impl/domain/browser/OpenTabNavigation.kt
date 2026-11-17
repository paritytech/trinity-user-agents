package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import java.net.URI

// With one tab per product, a link that names a page (another product's navigateTo, such as a meeting room)
// has to land on that page, while a bare product link (tab bar, catalog) leaves the tab where the user was.
internal fun loadsIntoOpenTab(currentUrl: String, requestedUrl: String): Boolean {
    if (requestedUrl == currentUrl) return false
    val requested = runCatching { URI(requestedUrl) }.getOrNull() ?: return false
    val path = requested.rawPath.orEmpty()
    return (path.isNotEmpty() && path != "/") || requested.rawQuery != null || requested.rawFragment != null
}
