package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.model.reserve

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.feature_xcm_api.config.model.AssetsXcmConfig
import io.paritytech.polkadotapp.feature_xcm_api.config.model.getReserve
import io.paritytech.polkadotapp.feature_xcm_api.converter.chain.ChainLocationConverter
import io.paritytech.polkadotapp.feature_xcm_api.converter.chain.chainLocationOf

internal class TokenReserveRegistry(
    private val xcmConfig: AssetsXcmConfig,
    private val chainRegistry: ChainRegistry,
    val chainLocationConverter: ChainLocationConverter,
) {
    suspend fun getReserve(chainAsset: Chain.Asset): TokenReserve {
        val reserve = xcmConfig.getReserve(chainAsset)
        val reserveChain = chainRegistry.getChain(reserve.reserveAssetId.chainId)
        return TokenReserve(
            reserveChainLocation = chainLocationConverter.chainLocationOf(reserveChain),
            tokenLocation = reserve.tokenLocation
        )
    }
}
