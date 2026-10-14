package io.paritytech.polkadotapp.tools_assethub_sdk_impl

import io.paritytech.polkadotapp.chains.di.RemoteSourceQualifier
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.origins.SignedOrigins
import io.paritytech.polkadotapp.feature_xcm_api.converter.LocationConverterFactory
import io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.DryRunApi
import io.paritytech.polkadotapp.feature_xcm_api.versions.detector.XcmVersionDetector
import io.paritytech.polkadotapp.tools_assethub_sdk_api.AssetHubSdk
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.AssetHubSdkOverridableData
import io.paritytech.polkadotapp.tools_assethub_sdk_impl.data.AssetConversionQuoter
import io.paritytech.polkadotapp.tools_assethub_sdk_impl.data.StandaloneAssetHubSdkOverridableData
import io.paritytech.polkadotapp.tools_assethub_sdk_impl.fee.AssetConversionFeePayment
import javax.inject.Inject

internal class AssetHubSwapSdkFactory @Inject constructor(
    private val extrinsicService: ExtrinsicService,
    @RemoteSourceQualifier private val remoteStorageSource: StorageDataSource,
    private val feePaymentFactory: AssetConversionFeePayment.Factory,
    private val locationConverterFactory: LocationConverterFactory,
    private val chainRegistry: ChainRegistry,
    private val defaultOverridableData: StandaloneAssetHubSdkOverridableData,
    private val quoterFactory: AssetConversionQuoter.Factory,
    private val xcmVersionDetector: XcmVersionDetector,
    private val dryRunApi: DryRunApi,
    private val signedOrigins: SignedOrigins,
) : AssetHubSdk.Factory {
    override suspend fun create(
        chainId: ChainId,
        overridableData: AssetHubSdkOverridableData?
    ): AssetHubSdk {
        val chain = chainRegistry.getChain(chainId)
        val multiLocationConverter = locationConverterFactory.createAssetLocationConverter()

        return RealAssetHubSwapSdk(
            chain = chain,
            remoteStorageSource = remoteStorageSource,
            multiLocationConverter = multiLocationConverter,
            overridableData = overridableData ?: defaultOverridableData,
            feePaymentFactory = feePaymentFactory,
            quoter = quoterFactory.create(chain, multiLocationConverter),
            xcmVersionDetector = xcmVersionDetector,
            extrinsicService = extrinsicService,
            chainRegistry = chainRegistry,
            dryRunApi = dryRunApi,
            signedOrigins = signedOrigins
        )
    }
}
