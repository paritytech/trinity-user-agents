package io.paritytech.polkadotapp.feature_products_impl.presentation.pocketAddCard

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import kotlinx.coroutines.flow.StateFlow

interface PocketAddCardContract {
    val state: StateFlow<LoadingState<PocketAddCardUiState>>

    fun onAddClicked()

    fun onCancelClicked()
}

@Immutable
data class PocketAddCardUiState(
    val productName: String,
    val title: String,
    val face: JsWidget,
    val imageResolver: JsImageResolver,
    val adding: Boolean,
)
