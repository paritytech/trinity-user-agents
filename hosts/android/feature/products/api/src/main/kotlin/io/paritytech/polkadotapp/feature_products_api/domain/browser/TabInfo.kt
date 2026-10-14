package io.paritytech.polkadotapp.feature_products_api.domain.browser

/** Display snapshot of one open product tab, for the tab bar. */
data class TabInfo(
    val id: Long,
    val title: String,
    val host: String?,
    val iconUrl: String?,
    val isActive: Boolean,
)
