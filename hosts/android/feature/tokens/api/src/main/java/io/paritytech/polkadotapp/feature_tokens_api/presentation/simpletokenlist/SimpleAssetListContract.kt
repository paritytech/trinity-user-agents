package io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.models.SimpleAssetListUiState
import kotlinx.coroutines.flow.StateFlow

interface SimpleAssetListContract {
    val state: StateFlow<SimpleAssetListUiState>

    fun backClicked()

    fun tokenClicked(asset: Chain.Asset)
}
