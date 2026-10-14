package io.paritytech.polkadotapp.feature_xcm_api.versions.detector

import io.novasama.substrate_sdk_android.runtime.definitions.types.RuntimeType
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion

interface XcmVersionDetector {
    suspend fun lowestPresentMultiLocationVersion(chainId: ChainId): XcmVersion?

    suspend fun lowestPresentMultiAssetsVersion(chainId: ChainId): XcmVersion?

    suspend fun lowestPresentMultiAssetVersion(chainId: ChainId): XcmVersion?

    suspend fun detectMultiLocationVersion(chainId: ChainId, multiLocationType: RuntimeType<*, *>?): XcmVersion?
}
