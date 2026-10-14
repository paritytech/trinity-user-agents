package io.paritytech.polkadotapp.feature_xcm_impl.converter.asset

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.feature_xcm_api.config.XcmConfigRepository
import io.paritytech.polkadotapp.feature_xcm_api.converter.LocationConverterFactory
import io.paritytech.polkadotapp.feature_xcm_api.converter.asset.ChainAssetLocationConverter
import io.paritytech.polkadotapp.feature_xcm_api.converter.chain.ChainLocationConverter
import io.paritytech.polkadotapp.feature_xcm_impl.converter.chain.RealChainLocationConverter
import javax.inject.Inject

class RealLocationConverterFactory @Inject constructor(
    private val xcmConfigRepository: XcmConfigRepository,
    private val chainRegistry: ChainRegistry,
) : LocationConverterFactory {
    override suspend fun createChainConverter(): ChainLocationConverter {
        val config = xcmConfigRepository.awaitXcmConfig()
        return RealChainLocationConverter(config.chains, chainRegistry)
    }

    override suspend fun createAssetLocationConverter(): ChainAssetLocationConverter {
        val config = xcmConfigRepository.awaitXcmConfig()
        val chainLocationConverter = RealChainLocationConverter(config.chains, chainRegistry)
        return RealChainAssetLocationConverter(config.assets, chainLocationConverter, chainRegistry)
    }
}
