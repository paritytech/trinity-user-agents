package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.model

import androidx.annotation.Keep
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.CrossChainTransfersDirectionsConfiguration
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.CrossChainTransfersDirectionsConfiguration.AssetTransfers
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.CrossChainTransfersDirectionsConfiguration.CustomTeleportEntry
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.configuration.CrossChainTransfersDirectionsConfiguration.TransferDestination

@Keep
internal class DynamicCrossChainTransfersConfigRemote(
    val chains: List<DynamicCrossChainOriginChainRemote>?,
    val customTeleports: List<CustomTeleportEntryRemote>?,
)

@Keep
internal class CustomTeleportEntryRemote(
    val originChain: String,
    val destChain: String,
    val originAsset: Int
)

@Keep
internal class DynamicCrossChainOriginChainRemote(
    val chainId: ChainId,
    val assets: List<DynamicCrossChainOriginAssetRemote>
)

@Keep
internal class DynamicCrossChainOriginAssetRemote(
    val assetId: Int,
    val xcmTransfers: List<DynamicXcmTransferRemote>,
)

@Keep
internal class DynamicXcmTransferRemote(
    val chainId: ChainId,
    val assetId: Int,
    val hasDeliveryFee: Boolean?,
    val supportsXcmExecute: Boolean?,
)

internal fun DynamicCrossChainTransfersConfigRemote.toDomain(): CrossChainTransfersDirectionsConfiguration {
    return CrossChainTransfersDirectionsConfiguration(
        chains = constructChains(chains),
        customTeleports = constructCustomTeleports(customTeleports)
    )
}

private fun constructCustomTeleports(
    customTeleports: List<CustomTeleportEntryRemote>?
): Set<CustomTeleportEntry> {
    return customTeleports.orEmpty().mapToSet { entry ->
        with(entry) {
            CustomTeleportEntry(FullChainAssetId(originChain, originAsset), destChain)
        }
    }
}

private fun constructChains(
    chains: List<DynamicCrossChainOriginChainRemote>?
): Map<ChainId, List<AssetTransfers>> {
    return chains.orEmpty().associateBy(
        keySelector = DynamicCrossChainOriginChainRemote::chainId,
        valueTransform = ::constructTransfersForChain
    )
}

private fun constructTransfersForChain(configRemote: DynamicCrossChainOriginChainRemote): List<AssetTransfers> {
    return configRemote.assets.map { assetConfig ->
        AssetTransfers(
            assetId = assetConfig.assetId,
            destinations = assetConfig.xcmTransfers.map { transfer ->
                TransferDestination(
                    fullChainAssetId = FullChainAssetId(transfer.chainId, transfer.assetId),
                    hasDeliveryFee = transfer.hasDeliveryFee ?: false,
                    supportsXcmExecute = transfer.supportsXcmExecute ?: false,
                )
            }
        )
    }
}
