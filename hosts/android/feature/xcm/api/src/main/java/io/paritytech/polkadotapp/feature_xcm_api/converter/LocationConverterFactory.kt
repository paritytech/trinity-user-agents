package io.paritytech.polkadotapp.feature_xcm_api.converter

import io.paritytech.polkadotapp.feature_xcm_api.converter.asset.ChainAssetLocationConverter
import io.paritytech.polkadotapp.feature_xcm_api.converter.chain.ChainLocationConverter

interface LocationConverterFactory {
    suspend fun createChainConverter(): ChainLocationConverter

    suspend fun createAssetLocationConverter(): ChainAssetLocationConverter
}
