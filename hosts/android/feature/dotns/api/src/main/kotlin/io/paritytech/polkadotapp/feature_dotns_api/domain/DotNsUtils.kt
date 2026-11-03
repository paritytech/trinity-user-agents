package io.paritytech.polkadotapp.feature_dotns_api.domain

import android.net.Uri
import androidx.core.net.toUri
import uniffi.truapi.parseProductLink

object DotNsUtils {
    /**
     * Whether [uri] points to a dotNS domain of the active network: a host under [tld],
     * or a web mirror whose root matches it (`coinflip.paseo.li` under `.paseo`).
     *
     * Expects a [Uri] with a scheme (e.g. `https://coinflip.dot/path`).
     * Bare hostnames without a scheme will return `false` since [Uri.getHost] returns null for them.
     */
    fun isDotDomain(uri: Uri, tld: DotNsTld): Boolean {
        if (uri.host == null) return false
        return parseProductLink(uri.toString(), tld.value) != null
    }

    /**
     * Normalize a dotNS domain [Uri]:
     * - Ensures `https://` scheme
     * - Converts a web mirror to its canonical dotNS name
     *
     * Returns `null` if [uri] is not a dotNS domain of the active network.
     */
    fun normalize(uri: Uri, tld: DotNsTld): Uri? {
        return parseProductLink(uri.toString(), tld.value)?.canonicalUrl?.toUri()
    }

    /**
     * Classify navigation from [origin] to [destination].
     *
     * - [DotNsNavigationType.EXTERNAL] if [destination] is not a dotNS domain of the active network
     * - [DotNsNavigationType.SAME_DOTNS_DOMAIN] if both resolve to the same dotNS host
     * - [DotNsNavigationType.CROSS_DOTNS_DOMAIN] otherwise (different dotNS hosts, or null origin)
     */
    fun classifyNavigation(origin: Uri?, destination: Uri, tld: DotNsTld): DotNsNavigationType {
        val normalizedDest = normalize(destination, tld)
            ?: return DotNsNavigationType.EXTERNAL

        if (origin == null) return DotNsNavigationType.CROSS_DOTNS_DOMAIN

        val normalizedOrigin = normalize(origin, tld)

        return if (normalizedOrigin?.host == normalizedDest.host) {
            DotNsNavigationType.SAME_DOTNS_DOMAIN
        } else {
            DotNsNavigationType.CROSS_DOTNS_DOMAIN
        }
    }
}

enum class DotNsNavigationType {
    SAME_DOTNS_DOMAIN, CROSS_DOTNS_DOMAIN, EXTERNAL
}
