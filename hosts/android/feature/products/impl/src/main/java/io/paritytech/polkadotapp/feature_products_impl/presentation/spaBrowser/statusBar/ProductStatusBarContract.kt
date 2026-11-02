package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar

import kotlinx.coroutines.flow.StateFlow
import uniffi.truapi.ProductId

interface ProductStatusBarContract {
    val state: StateFlow<ProductStatusBarUiState>

    fun onProceedInChatClick()

    fun onReloadClick()

    fun onShareClick()

    fun onCloseAppClick()
}

data class ProductStatusBarUiState(
    val title: String,
    val menu: ProductMenu,
)

sealed interface ProductMenu {
    data object WebPage : ProductMenu

    data class Product(val id: ProductId, val chat: ProductChat) : ProductMenu
}

sealed interface ProductChat {
    data object Served : ProductChat

    data object NotServed : ProductChat

    data object Unresolved : ProductChat
}
