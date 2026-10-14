package io.paritytech.polkadotapp.feature_products_api.domain.browser

/**
 * Display snapshot of the single live browser session, for the beard chrome to render.
 * Carries no rendering internals (no WebView) — those stay in the products impl.
 */
data class BrowserSessionInfo(
    val title: String,
    val host: String?,
    val url: String,
    val isLoading: Boolean,
    val loadFraction: Float?,
)
