package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import java.net.URI

/**
 * The page a debug product is served from, carrying the query of [launchUrl].
 *
 * A card opens its product with `?card=<id>`, which is how the page knows which card it sits under.
 */
fun debugServedUrl(appUrl: String, launchUrl: String): String {
    val launchQuery = URI(launchUrl).rawQuery ?: return appUrl
    val served = URI(appUrl)

    return buildString {
        append(appUrl.substringBefore('?').substringBefore('#'))
        append('?')
        served.rawQuery?.let { append(it).append('&') }
        append(launchQuery)
        served.rawFragment?.let { append('#').append(it) }
    }
}
