package io.paritytech.polkadotapp.chains.multiNetwork

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.common.utils.removeHexPrefix
import kotlin.text.get

@JvmInline
value class ChainsById(val value: Map<ChainId, Chain>) : Map<ChainId, Chain> by value {
    override operator fun get(key: ChainId): Chain? {
        return value[key.removeHexPrefix()]
    }
}

@Suppress("NOTHING_TO_INLINE")
inline fun Map<ChainId, Chain>.asChainsById(): ChainsById {
    return ChainsById(this)
}

fun ChainsById.assetOrNull(id: FullChainAssetId): Chain.Asset? {
    return get(id.chainId)?.assetsById?.get(id.assetId)
}

fun ChainsById.chainWithAssetOrNull(id: FullChainAssetId): ChainWithAsset? {
    val chain = get(id.chainId) ?: return null
    val asset = chain.assetsById[id.assetId] ?: return null

    return ChainWithAsset(chain, asset)
}
