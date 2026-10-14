package io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.feature_tokens_api.domain.AssetDisplayMapper
import io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.models.SimpleAssetListUiState
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.stateIn

abstract class SimpleAssetListViewModel(
    private val assetDisplayMapper: AssetDisplayMapper,
) : BaseViewModel(), SimpleAssetListContract {
    abstract suspend fun assets(): List<Chain.Asset>

    override val state = flowOf {
        val displays = assets().mapNotNull { assetDisplayMapper.displayOf(it) }
        SimpleAssetListUiState(displays)
    }
        .stateIn(
            scope = this,
            started = SharingStarted.Lazily,
            initialValue = SimpleAssetListUiState()
        )
}
