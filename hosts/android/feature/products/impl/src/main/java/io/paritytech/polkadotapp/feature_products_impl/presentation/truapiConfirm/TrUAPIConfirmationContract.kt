package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiConfirm

import androidx.annotation.StringRes
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import kotlinx.collections.immutable.ImmutableList
import kotlinx.coroutines.flow.StateFlow

interface TrUAPIConfirmationContract {
    val state: StateFlow<LoadingState<TrUAPIConfirmationUiState>>

    fun onApproveClicked()

    fun onRejectClicked()
}

data class TrUAPIConfirmationUiState(
    @StringRes val titleRes: Int,
    val productId: String,
    val details: ImmutableList<TrUAPIConfirmationDetail>,
)

data class TrUAPIConfirmationDetail(
    @StringRes val labelRes: Int,
    val value: DetailValue,
)

sealed interface DetailValue {
    class Text(val text: String) : DetailValue
    class Resource(@StringRes val res: Int) : DetailValue
}
