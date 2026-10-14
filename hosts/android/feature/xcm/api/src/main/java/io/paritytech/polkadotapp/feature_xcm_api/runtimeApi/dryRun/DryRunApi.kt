package io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.OriginCaller
import io.paritytech.polkadotapp.chains.network.binding.ScaleResult
import io.paritytech.polkadotapp.feature_xcm_api.message.VersionedRawXcmMessage
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.model.CallDryRunEffects
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.model.DryRunEffectsResultErr
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.model.XcmDryRunEffects
import io.paritytech.polkadotapp.feature_xcm_api.versions.VersionedXcmLocation
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion

interface DryRunApi {
    suspend fun dryRunXcm(
        originLocation: VersionedXcmLocation,
        xcm: VersionedRawXcmMessage,
        chainId: ChainId
    ): Result<ScaleResult<XcmDryRunEffects, DryRunEffectsResultErr>>

    suspend fun dryRunCall(
        originCaller: OriginCaller,
        call: GenericCall.Instance,
        chainId: ChainId,
        xcmResultsVersion: XcmVersion = XcmVersion.V4
    ): Result<ScaleResult<CallDryRunEffects, DryRunEffectsResultErr>>
}
