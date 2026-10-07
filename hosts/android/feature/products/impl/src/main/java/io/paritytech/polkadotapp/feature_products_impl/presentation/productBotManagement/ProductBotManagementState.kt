package io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

@Immutable
data class ProductBotManagementState(
    val products: List<ProductUiModel> = emptyList(),
    val dialogState: ProductDialogState = ProductDialogState.None,
    val tldSuffix: String = "",
)

@Immutable
data class ProductUiModel(
    val id: ProductId,
    val name: String,
    val appUrl: String,
)

@Immutable
sealed interface ProductDialogState {
    data object None : ProductDialogState

    data class Form(
        val productId: String? = null,
        val dotNsName: String = "",
        val workerUrl: String = "",
        // A Pocket card on the worker. Optional: a worker can be useful with only chat on it.
        val cardId: String = "",
        val cardTitle: String = "",
        val previewUrl: String = "",
        // Moves only this product's worker and chat onto the core under wasmi.
        val runsOnWasmi: Boolean = false,
        val isSubmitting: Boolean = false,
    ) : ProductDialogState
}
