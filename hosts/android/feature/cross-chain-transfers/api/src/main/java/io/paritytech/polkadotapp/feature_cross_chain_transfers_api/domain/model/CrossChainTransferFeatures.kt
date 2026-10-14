package io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.model

class CrossChainTransferFeatures(
    val supportsXcmExecute: Boolean,
    val hasDeliveryFees: Boolean,
)

fun CrossChainTransferFeatures.canPayCrossChainFeesFromTransferringAmount(): Boolean {
    // By default, delivery fees are not payable in transferring amount
    return !hasDeliveryFees ||
        // ... but xcm execute allows to workaround it
        supportsXcmExecute
}

fun CrossChainTransferFeatures.canTransferOutWholeBalance(): Boolean {
    // Precisely speaking just checking for delivery fees is not enough
    // AssetTransactor on origin should also use Preserve transfers when executing TransferAssets instruction
    // However it is much harder to check and there are no chains yet that have limitations on AssetTransactor level
    // but don't have delivery fees, so we only check for delivery fees
    return !hasDeliveryFees ||
        // When direction has delivery fees, xcm execute can be used to pay them from holding, thus allowing to transfer whole balance
        // and also workaround AssetTransactor issue as "Withdraw" instruction doesn't use Preserve transfers but rather use burn
        supportsXcmExecute
}
