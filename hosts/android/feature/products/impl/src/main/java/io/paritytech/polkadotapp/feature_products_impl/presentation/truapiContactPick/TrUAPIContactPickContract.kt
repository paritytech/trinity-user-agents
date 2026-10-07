package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiContactPick

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import kotlinx.collections.immutable.ImmutableList
import kotlinx.coroutines.flow.StateFlow

interface TrUAPIContactPickContract {
    val state: StateFlow<LoadingState<TrUAPIContactPickUiState>>

    fun onContactClicked(index: Int)

    fun onDismissClicked()

    fun onShown()
}

data class TrUAPIContactPickUiState(
    val productId: String,
    val contacts: ImmutableList<TrUAPIContactPickRow>,
)

/**
 * A pickable row. [index] is the position in the context's own list, which is
 * what the answer is keyed by: a display name is not unique and the account
 * never belongs on screen. A null [name] is a contact the app holds no name
 * for, which the row labels rather than hides.
 */
data class TrUAPIContactPickRow(
    val index: Int,
    val name: String?,
)
