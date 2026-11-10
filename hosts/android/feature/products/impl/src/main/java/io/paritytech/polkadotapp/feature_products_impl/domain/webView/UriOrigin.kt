package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.net.Uri

fun Uri.originOrNull(): String? {
    val scheme = scheme ?: return null
    val host = host ?: return null
    val port = port.takeIf { it != -1 }?.let { ":$it" }.orEmpty()
    return "$scheme://$host$port"
}
