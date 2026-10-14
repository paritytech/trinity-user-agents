package io.paritytech.polkadotapp.feature_xcm_impl.runtimeApi.dryRun

import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.chains.call.MultiChainRuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.RuntimeCallsApi
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.provideContext
import io.paritytech.polkadotapp.chains.network.binding.OriginCaller
import io.paritytech.polkadotapp.chains.network.binding.ScaleResult
import io.paritytech.polkadotapp.feature_xcm_api.message.VersionedRawXcmMessage
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.DryRunApi
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.model.CallDryRunEffects
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.model.DryRunEffectsResultErr
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.model.XcmDryRunEffects
import io.paritytech.polkadotapp.feature_xcm_api.versions.VersionedXcmLocation
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion
import io.paritytech.polkadotapp.feature_xcm_api.versions.toEncodableInstance
import javax.inject.Inject

class RealDryRunApi @Inject constructor(
    private val multiChainRuntimeCallsApi: MultiChainRuntimeCallsApi
) : DryRunApi {
    override suspend fun dryRunXcm(
        originLocation: VersionedXcmLocation,
        xcm: VersionedRawXcmMessage,
        chainId: ChainId
    ): Result<ScaleResult<XcmDryRunEffects, DryRunEffectsResultErr>> {
        return multiChainRuntimeCallsApi.forChain(chainId).dryRunXcm(xcm, originLocation)
    }

    override suspend fun dryRunCall(
        originCaller: OriginCaller,
        call: GenericCall.Instance,
        chainId: ChainId,
        xcmResultsVersion: XcmVersion
    ): Result<ScaleResult<CallDryRunEffects, DryRunEffectsResultErr>> {
        return multiChainRuntimeCallsApi.forChain(chainId).dryRunCall(originCaller, call, xcmResultsVersion)
    }

    private suspend fun RuntimeCallsApi.dryRunXcm(
        xcm: VersionedRawXcmMessage,
        origin: VersionedXcmLocation,
    ): Result<ScaleResult<XcmDryRunEffects, DryRunEffectsResultErr>> {
        return runCatching {
            call(
                section = "DryRunApi",
                method = "dry_run_xcm",
                arguments = mapOf(
                    "origin_location" to origin.toEncodableInstance(),
                    "xcm" to xcm.toEncodableInstance()
                ),
                returnBinding = {
                    runtime.provideContext {
                        ScaleResult.bind(
                            dynamicInstance = it,
                            bindOk = { ok -> XcmDryRunEffects.bind(ok) },
                            bindError = DryRunEffectsResultErr::bind
                        )
                    }
                }
            )
        }
    }

    private suspend fun RuntimeCallsApi.dryRunCall(
        originCaller: OriginCaller,
        call: GenericCall.Instance,
        xcmResultsVersion: XcmVersion,
    ): Result<ScaleResult<CallDryRunEffects, DryRunEffectsResultErr>> {
        return runCatching {
            call(
                section = "DryRunApi",
                method = "dry_run_call",
                arguments = mapOf(
                    "origin" to originCaller.toEncodableInstance(),
                    "call" to call,
                    "result_xcms_version" to xcmResultsVersion.version.toBigInteger()
                ),
                returnBinding = {
                    runtime.provideContext {
                        ScaleResult.bind(
                            dynamicInstance = it,
                            bindOk = { CallDryRunEffects.bind(it) },
                            bindError = DryRunEffectsResultErr::bind
                        )
                    }
                }
            )
        }
    }
}
