package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.domain.error.ProductResolutionError
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.ProductStatusBarUiState
import kotlinx.coroutines.flow.StateFlow

interface SpaBrowserContract {
    val state: StateFlow<SpaBrowserUiState>

    fun onBackPressed()

    fun onRefresh()

    fun onProceedInChatClick()

    fun onShareClick()

    fun onCloseAppClick()

    fun onScreenStarted()
}

data class SpaBrowserUiState(
    val statusBar: ProductStatusBarUiState? = null,
    val loadProgress: DotNsLoadProgress = DotNsLoadProgress.Idle,
    val pageState: SpaBrowserPageState = SpaBrowserPageState.Content,
    val isDevServer: Boolean = false,
)

/** What the content area shows. Anything still in flight is [Content] with [SpaBrowserUiState.loadProgress] running. */
sealed interface SpaBrowserPageState {
    data object Content : SpaBrowserPageState
    data object NoAppSurface : SpaBrowserPageState
    data class Failed(val error: ProductResolutionError) : SpaBrowserPageState
}
