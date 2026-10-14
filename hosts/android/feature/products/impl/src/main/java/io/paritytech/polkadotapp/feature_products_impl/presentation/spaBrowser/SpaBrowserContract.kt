package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.domain.error.ProductResolutionError
import kotlinx.coroutines.flow.StateFlow

interface SpaBrowserContract {
    val state: StateFlow<SpaBrowserUiState>

    fun onBackPressed()
}

data class SpaBrowserUiState(
    val title: String? = "",
    val subtitle: String? = "",
    val loadProgress: DotNsLoadProgress = DotNsLoadProgress.Idle,
    val pageState: SpaBrowserPageState = SpaBrowserPageState.Content,
)

/** What the content area shows. Anything still in flight is [Content] with [SpaBrowserUiState.loadProgress] running. */
sealed interface SpaBrowserPageState {
    data object Content : SpaBrowserPageState
    data object NoAppSurface : SpaBrowserPageState
    data class Failed(val error: ProductResolutionError) : SpaBrowserPageState
}
