package io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.flow.StateFlow

interface ProductBotManagementContract {
    val state: StateFlow<ProductBotManagementState>

    fun onBackClick()

    fun onAddProductClick()

    fun onProductClick(productId: ProductId)

    fun onEditProductClick(productId: String)

    fun onDeleteProductClick(productId: String)

    fun onDialogDismiss()

    fun onDotNsDomainChanged(dotNsDomain: String)

    fun onWorkerUrlChanged(url: String)

    fun onCardIdChanged(cardId: String)

    fun onCardTitleChanged(title: String)

    fun onPreviewUrlChanged(url: String)

    fun onAppUrlChanged(url: String)

    fun onOpenWithFaceAwayChanged(faceAway: Boolean)

    fun onDialogConfirm()
}
