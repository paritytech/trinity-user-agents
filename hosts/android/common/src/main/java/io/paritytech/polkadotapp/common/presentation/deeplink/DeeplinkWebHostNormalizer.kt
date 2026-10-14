package io.paritytech.polkadotapp.common.presentation.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler.Companion.APP_SCHEME
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler.Companion.WEB_HTTPS_SCHEME
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler.Companion.WEB_HTTP_SCHEME
import io.paritytech.polkadotapp.common.utils.DOT_WEB_MIRROR_SUFFIXES
import io.paritytech.polkadotapp.common.utils.isDotWebMirrorHost
import io.paritytech.polkadotapp.common.utils.toCanonicalDotHost

private val BARE_DEEPLINK_HOSTS = DOT_WEB_MIRROR_SUFFIXES.map { it.removePrefix(".") }.toSet()

private fun webDeeplinkHostOrNull(scheme: String?, host: String?): String? =
    host?.takeIf { scheme == WEB_HTTPS_SCHEME || scheme == WEB_HTTP_SCHEME }

/**
 * Rewrites a `dot.li`/`paseo.li` web App Link to the app scheme: a subdomain is a product
 * (`product.dot.li/a` -> `polkadotapp://product.dot/a`), a bare host carries the action in the
 * path (`dot.li/pair` -> `polkadotapp://pair`). Null if not a web deeplink host.
 */
internal fun Uri.toAppSchemeDeeplinkOrNull(): Uri? {
    dotProductHost(scheme, host)?.let { productHost ->
        return buildUpon().scheme(APP_SCHEME).authority(productHost).build()
    }

    val segments = pathSegments.orEmpty()
    val action = bareHostDeeplinkAction(scheme, host, segments) ?: return null
    val rest = segments.drop(1)
    return buildUpon()
        .scheme(APP_SCHEME)
        .authority(action)
        .path(if (rest.isEmpty()) "" else rest.joinToString(separator = "/", prefix = "/"))
        .build()
}

internal fun bareHostDeeplinkAction(scheme: String?, host: String?, pathSegments: List<String>): String? {
    val webHost = webDeeplinkHostOrNull(scheme, host) ?: return null
    if (webHost !in BARE_DEEPLINK_HOSTS) return null

    return pathSegments.firstOrNull()
}

internal fun dotProductHost(scheme: String?, host: String?): String? {
    val webHost = webDeeplinkHostOrNull(scheme, host) ?: return null
    if (!isDotWebMirrorHost(webHost)) return null

    return webHost.toCanonicalDotHost()
}
