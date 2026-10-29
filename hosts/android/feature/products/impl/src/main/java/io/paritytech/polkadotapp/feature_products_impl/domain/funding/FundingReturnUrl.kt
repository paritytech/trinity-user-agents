package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import java.net.URI

/**
 * Whether [url] is the page a provider's external frame finishes on: the same scheme, host, port and path as
 * [returnUrl], whatever query and fragment the onramp appends with its result.
 */
fun reachesReturnUrl(url: String, returnUrl: String): Boolean {
    val reached = runCatching { URI(url) }.getOrNull() ?: return false
    val expected = runCatching { URI(returnUrl) }.getOrNull() ?: return false

    return reached.scheme.equals(expected.scheme, ignoreCase = true) &&
        reached.host.equals(expected.host, ignoreCase = true) &&
        reached.effectivePort() == expected.effectivePort() &&
        reached.rawPath.ifEmpty { "/" } == expected.rawPath.ifEmpty { "/" }
}

private fun URI.effectivePort(): Int = when {
    port != -1 -> port
    scheme.equals("https", ignoreCase = true) -> 443
    scheme.equals("http", ignoreCase = true) -> 80
    else -> -1
}
