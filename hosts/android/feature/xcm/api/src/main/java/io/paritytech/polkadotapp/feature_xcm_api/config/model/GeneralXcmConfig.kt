package io.paritytech.polkadotapp.feature_xcm_api.config.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.chains.util.fullId
import io.paritytech.polkadotapp.chains.util.normalizeSymbol
import io.paritytech.polkadotapp.common.domain.model.ParaId

class GeneralXcmConfig(
    val chains: ChainXcmConfig,
    val assets: AssetsXcmConfig
)

class ChainXcmConfig(
    val parachainIds: Map<ChainId, ParaId>
)

class AssetsXcmConfig(
    val reservesById: Map<ChainAssetReserveId, ChainAssetReserveConfig>,
    // By default, asset reserve id is equal to its symbol
    // This mapping allows to override that for cases like multiple reserves (Statemine & Polkadot for DOT)
    val assetToReserveIdOverrides: Map<FullChainAssetId, ChainAssetReserveId>,
)

fun AssetsXcmConfig.getReserve(chainAsset: Chain.Asset): ChainAssetReserveConfig {
    val reserveId = getReserveId(chainAsset)
    val reserve = reservesById.getValue(reserveId)
    return reserve
}

private fun AssetsXcmConfig.getReserveId(chainAsset: Chain.Asset): ChainAssetReserveId {
    return assetToReserveIdOverrides[chainAsset.fullId] ?: chainAsset.normalizeSymbol()
}
