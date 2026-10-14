package io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetDisplay

@Immutable
class SimpleAssetListUiState(
    val assets: List<AssetDisplay> = listOf(),
)
