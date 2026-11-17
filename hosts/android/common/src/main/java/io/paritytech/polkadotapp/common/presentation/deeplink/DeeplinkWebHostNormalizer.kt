package io.paritytech.polkadotapp.common.presentation.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler.Companion.APP_SCHEME
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler.Companion.WEB_HTTPS_SCHEME
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler.Companion.WEB_HTTP_SCHEME
import uniffi.truapi.Stack

private fun webDeeplinkHostOrNull(scheme: String?, host: String?): String? =
    host?.takeIf { scheme == WEB_HTTPS_SCHEME || scheme == WEB_HTTP_SCHEME }

/**
 * Rewrites a share-domain web App Link of the [stack] to the app scheme: a subdomain is a product
 * (`product.dot.li/a` -> `polkadotapp://product.dot/a`), a bare host carries the action in the
 * path (`dot.li/pair` -> `polkadotapp://pair`). Null if not a web deeplink host the stack serves.
 */
internal fun Uri.toAppSchemeDeeplinkOrNull(stack: Stack): Uri? {
    dotProductHost(scheme, host, stack)?.let { productHost ->
        return buildUpon().scheme(APP_SCHEME).authority(productHost).build()
    }

    val segments = pathSegments.orEmpty()
    val action = bareHostDeeplinkAction(scheme, host, segments, stack) ?: return null
    val rest = segments.drop(1)
    return buildUpon()
        .scheme(APP_SCHEME)
        .authority(action)
        .path(if (rest.isEmpty()) "" else rest.joinToString(separator = "/", prefix = "/"))
        .build()
}

internal fun bareHostDeeplinkAction(
    scheme: String?,
    host: String?,
    pathSegments: List<String>,
    stack: Stack,
): String? {
    val webHost = webDeeplinkHostOrNull(scheme, host) ?: return null
    if (webHost !in stack.productShareHosts()) return null

    return pathSegments.firstOrNull()
}

internal fun dotProductHost(scheme: String?, host: String?, stack: Stack): String? {
    val webHost = webDeeplinkHostOrNull(scheme, host) ?: return null
    return stack.parseProductShareHost(webHost)?.value
}
