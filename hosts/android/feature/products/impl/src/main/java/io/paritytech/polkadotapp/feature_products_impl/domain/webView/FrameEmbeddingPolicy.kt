package io.paritytech.polkadotapp.feature_products_impl.domain.webView

// Response headers stamped on a product's main document to gate iframe embedding. When iframes are
// disallowed we serve a Content-Security-Policy that forbids nested browsing contexts; the WebView
// engine enforces it directly, so the gate does not depend on inspecting per-request headers.
internal fun frameEmbeddingResponseHeaders(allowIframes: Boolean): Map<String, String> =
    if (allowIframes) {
        emptyMap()
    } else {
        mapOf("Content-Security-Policy" to "frame-src 'none'; object-src 'none'")
    }
