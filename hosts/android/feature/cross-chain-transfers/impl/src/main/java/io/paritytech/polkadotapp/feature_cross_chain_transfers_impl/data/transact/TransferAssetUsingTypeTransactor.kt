package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.transact

import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.GenericCall
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.withRuntime
import io.paritytech.polkadotapp.chains.util.composeCall
import io.paritytech.polkadotapp.chains.util.xcmPalletName
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.CrossChainTransfer
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model.withdrawAmount
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.CrossChainTransferConfiguration
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.assetLocationOnOrigin
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.destinationChainLocationOnOrigin
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.originChainId
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.model.reserve.XcmTransferType
import io.paritytech.polkadotapp.feature_xcm_api.asset.MultiAsset
import io.paritytech.polkadotapp.feature_xcm_api.asset.MultiAssetFilter
import io.paritytech.polkadotapp.feature_xcm_api.asset.MultiAssetId
import io.paritytech.polkadotapp.feature_xcm_api.asset.MultiAssets
import io.paritytech.polkadotapp.feature_xcm_api.builder.XcmBuilder
import io.paritytech.polkadotapp.feature_xcm_api.builder.buildXcmWithoutFeesMeasurement
import io.paritytech.polkadotapp.feature_xcm_api.message.VersionedXcmMessage
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion
import io.paritytech.polkadotapp.feature_xcm_api.versions.detector.XcmVersionDetector
import io.paritytech.polkadotapp.feature_xcm_api.versions.orDefault
import io.paritytech.polkadotapp.feature_xcm_api.versions.toEncodableInstance
import io.paritytech.polkadotapp.feature_xcm_api.versions.versionedXcm
import io.paritytech.polkadotapp.feature_xcm_api.weight.WeightLimit
import javax.inject.Inject

internal class TransferAssetUsingTypeTransactor @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val xcmBuilderFactory: XcmBuilder.Factory,
    private val xcmVersionDetector: XcmVersionDetector,
) {
    suspend fun composeCall(
        configuration: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer,
        forceXcmVersion: XcmVersion? = null
    ): GenericCall.Instance {
        val totalTransferAmount = transfer.withdrawAmount
        val multiAsset = MultiAsset.from(configuration.assetLocationOnOrigin(), totalTransferAmount)
        val multiAssetId = MultiAssetId(configuration.assetLocationOnOrigin())

        val originChain = transfer.direction.from.chain

        val multiLocationVersion = forceXcmVersion ?: xcmVersionDetector.lowestPresentMultiLocationVersion(originChain.id).orDefault()
        val multiAssetVersion = forceXcmVersion ?: xcmVersionDetector.lowestPresentMultiAssetVersion(originChain.id).orDefault()

        val transferTypeParam = configuration.transferTypeParam(multiAssetVersion)

        return chainRegistry.withRuntime(configuration.originChainId) {
            composeCall(
                moduleName = runtime.metadata.xcmPalletName(),
                callName = "transfer_assets_using_type_and_then",
                arguments = mapOf(
                    "dest" to configuration.destinationChainLocationOnOrigin().versionedXcm(multiLocationVersion).toEncodableInstance(),
                    "assets" to MultiAssets(multiAsset).versionedXcm(multiAssetVersion).toEncodableInstance(),
                    "assets_transfer_type" to transferTypeParam,
                    "remote_fees_id" to multiAssetId.versionedXcm(multiAssetVersion).toEncodableInstance(),
                    "fees_transfer_type" to transferTypeParam,
                    "custom_xcm_on_dest" to constructCustomXcmOnDest(configuration, transfer, multiLocationVersion).toEncodableInstance(),
                    "weight_limit" to WeightLimit.Unlimited.toEncodableInstance()
                )
            )
        }
    }

    private fun CrossChainTransferConfiguration.transferTypeParam(locationXcmVersion: XcmVersion): Any {
        return when (val type = transferType) {
            is XcmTransferType.Teleport -> DictEnum.Entry("Teleport", null)

            is XcmTransferType.Reserve.Destination -> DictEnum.Entry("DestinationReserve", null)

            is XcmTransferType.Reserve.Origin -> DictEnum.Entry("LocalReserve", null)

            is XcmTransferType.Reserve.Remote -> {
                val reserveChainRelative = type.remoteReserveLocation.location.fromPointOfViewOf(originChainLocation.location)
                val remoteReserveEncodable = reserveChainRelative.versionedXcm(locationXcmVersion).toEncodableInstance()

                DictEnum.Entry("RemoteReserve", remoteReserveEncodable)
            }
        }
    }

    private suspend fun constructCustomXcmOnDest(
        configuration: CrossChainTransferConfiguration,
        transfer: CrossChainTransfer,
        minDetectedXcmVersion: XcmVersion
    ): VersionedXcmMessage {
        return xcmBuilderFactory.buildXcmWithoutFeesMeasurement(
            initial = configuration.originChainLocation,
            // singleCounted is only available from V3
            xcmVersion = minDetectedXcmVersion.coerceAtLeast(XcmVersion.V3)
        ) {
            depositAsset(MultiAssetFilter.singleCounted(), transfer.recipient)
        }
    }
}
