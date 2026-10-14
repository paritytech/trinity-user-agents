package io.paritytech.polkadotapp.tools_assethub_sdk_impl.data

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion
import io.paritytech.polkadotapp.feature_xcm_api.versions.detector.XcmVersionDetector
import io.paritytech.polkadotapp.feature_xcm_api.versions.orDefault
import io.paritytech.polkadotapp.tools_assethub_sdk_impl.data.api.assetConversionAssetIdType

suspend fun XcmVersionDetector.detectAssetIdXcmVersion(
    chainId: ChainId,
    runtime: RuntimeSnapshot
): XcmVersion {
    val assetIdType = runtime.metadata.assetConversionAssetIdType()
    return detectMultiLocationVersion(chainId, assetIdType).orDefault()
}
