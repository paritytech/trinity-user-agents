package io.paritytech.polkadotapp.feature_tokens_api.presentation.model

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain

@Immutable
data class AssetDisplay(
    val asset: Chain.Asset,
    val displayId: AssetDisplayId,
)

enum class AssetDisplayId {
    DOT, USDC, USDT, PAS
}
