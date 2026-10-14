package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.requests.StorageSharedRequestsBuilderFactory
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.SignedOrigins
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.DryRunApi
import io.paritytech.polkadotapp.tools_hydration_sdk_api.HydrationSwapOverridableData
import io.paritytech.polkadotapp.tools_hydration_sdk_api.HydrationSwapSdk
import io.paritytech.polkadotapp.tools_hydration_sdk_api.swap.model.WeightSpec
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.RealHydrationSwapSdk
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetIdConverter
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.data.StandaloneHydrationSwapOverridableData
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.HydrationFastLookupCustomFeeCapabilityFactory
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.HydrationFeePayment
import kotlinx.coroutines.CoroutineScope
import javax.inject.Inject

internal class HydrationSwapSdkFactory @Inject constructor(
    private val extrinsicService: ExtrinsicService,
    private val quotingFactories: Set<@JvmSuppressWildcards HydrationSwapSource.Factory>,
    private val feePaymentFactory: HydrationFeePayment.Factory,
    private val hydraDxAssetIdConverter: HydraDxAssetIdConverter,
    private val chainRegistry: ChainRegistry,
    private val defaultOverridableData: StandaloneHydrationSwapOverridableData,
    private val sharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory,
    private val feeCapabilityFactory: HydrationFastLookupCustomFeeCapabilityFactory,
    private val dryRunApi: DryRunApi,
    private val signedOrigins: SignedOrigins,
) : HydrationSwapSdk.Factory {
    override suspend fun create(
        chain: Chain,
        coroutineScope: CoroutineScope,
        weightSpec: WeightSpec,
        overridableData: HydrationSwapOverridableData?
    ): HydrationSwapSdk {
        return RealHydrationSwapSdk(
            chain = chain,
            weightSpec = weightSpec,
            extrinsicService = extrinsicService,
            overridableData = overridableData ?: defaultOverridableData,
            quotingFactories = quotingFactories,
            feePaymentFactory = feePaymentFactory,
            hydraDxAssetIdConverter = hydraDxAssetIdConverter,
            feeCapabilityFactory = feeCapabilityFactory,
            sharedRequestsBuilderFactory = sharedRequestsBuilderFactory,
            chainRegistry = chainRegistry,
            dryRunApi = dryRunApi,
            signedOrigins = signedOrigins
        )
    }
}
