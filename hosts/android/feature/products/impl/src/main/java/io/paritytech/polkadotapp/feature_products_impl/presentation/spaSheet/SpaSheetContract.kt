package io.paritytech.polkadotapp.feature_products_impl.presentation.spaSheet

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import kotlinx.coroutines.flow.StateFlow

interface SpaSheetContract {
    val state: StateFlow<SpaSheetUiState>

    fun onBackPressed()
}

data class SpaSheetUiState(
    val loadProgress: DotNsLoadProgress = DotNsLoadProgress.Idle,
    val isContentVisible: Boolean = false,
)
