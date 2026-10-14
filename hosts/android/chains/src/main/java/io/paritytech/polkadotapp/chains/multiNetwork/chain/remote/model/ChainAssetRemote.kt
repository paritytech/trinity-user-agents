package io.paritytech.polkadotapp.chains.multiNetwork.chain.remote.model

data class ChainAssetRemote(
    val assetId: Int,
    val symbol: String,
    val precision: Int,
    val priceId: String?,
    val name: String?,
    val type: String?,
    val typeExtras: Map<String, Any?>?,
)
