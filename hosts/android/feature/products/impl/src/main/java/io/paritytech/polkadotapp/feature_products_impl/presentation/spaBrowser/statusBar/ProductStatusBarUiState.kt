package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar

import uniffi.truapi.ProductId

data class ProductStatusBarUiState(
    val title: String,
    val menu: ProductMenu,
)

sealed interface ProductMenu {
    data object WebPage : ProductMenu

    data class Product(val id: ProductId, val servesChat: Boolean) : ProductMenu
}
