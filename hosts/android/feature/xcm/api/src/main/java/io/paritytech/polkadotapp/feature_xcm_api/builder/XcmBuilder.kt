package io.paritytech.polkadotapp.feature_xcm_api.builder

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_xcm_api.asset.MultiAsset
import io.paritytech.polkadotapp.feature_xcm_api.asset.MultiAssetFilter
import io.paritytech.polkadotapp.feature_xcm_api.asset.MultiAssetFilter.Wild.All
import io.paritytech.polkadotapp.feature_xcm_api.asset.MultiAssetId
import io.paritytech.polkadotapp.feature_xcm_api.asset.MultiAssets
import io.paritytech.polkadotapp.feature_xcm_api.asset.intoMultiAssets
import io.paritytech.polkadotapp.feature_xcm_api.asset.withAmount
import io.paritytech.polkadotapp.feature_xcm_api.builder.fees.MeasureXcmFees
import io.paritytech.polkadotapp.feature_xcm_api.builder.fees.PayFeesMode
import io.paritytech.polkadotapp.feature_xcm_api.builder.fees.UnsupportedMeasureXcmFees
import io.paritytech.polkadotapp.feature_xcm_api.message.VersionedXcmMessage
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.AbsoluteMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.AssetLocation
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.ChainLocation
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion
import io.paritytech.polkadotapp.feature_xcm_api.weight.WeightLimit

interface XcmBuilder : XcmContext {
    interface Factory {
        fun create(
            initial: ChainLocation,
            xcmVersion: XcmVersion,
            measureXcmFees: MeasureXcmFees
        ): XcmBuilder
    }

    fun payFees(payFeesMode: PayFeesMode)

    fun withdrawAsset(assets: MultiAssets)

    fun buyExecution(fees: MultiAsset, weightLimit: WeightLimit)

    // We only support depositing to a accountId. We might extend it in the future with no issues
    // but we keep the support limited to simplify implementation
    fun depositAsset(assets: MultiAssetFilter, beneficiary: AccountId)

    // Performs context change
    fun transferReserveAsset(assets: MultiAssets, dest: ChainLocation)

    // Performs context change
    fun initiateReserveWithdraw(assets: MultiAssetFilter, reserve: ChainLocation)

    // Performs context change
    fun depositReserveAsset(assets: MultiAssetFilter, dest: ChainLocation)

    fun initiateTeleport(assets: MultiAssetFilter, dest: ChainLocation)

    suspend fun build(): VersionedXcmMessage
}

/**
 * Can be used when `payFees` is not expected to be used
 */
fun XcmBuilder.Factory.createWithoutFeesMeasurement(
    initial: ChainLocation,
    xcmVersion: XcmVersion,
): XcmBuilder {
    return create(initial, xcmVersion, UnsupportedMeasureXcmFees())
}

suspend fun XcmBuilder.Factory.buildXcmWithoutFeesMeasurement(
    initial: ChainLocation,
    xcmVersion: XcmVersion,
    building: XcmBuilder.() -> Unit
): VersionedXcmMessage {
    return createWithoutFeesMeasurement(initial, xcmVersion)
        .apply(building)
        .build()
}

fun XcmBuilder.withdrawAsset(asset: AbsoluteMultiLocation, amount: Balance) {
    withdrawAsset(MultiAsset.from(asset.relativeToLocal(), amount).intoMultiAssets())
}

fun XcmBuilder.transferReserveAsset(asset: AbsoluteMultiLocation, amount: Balance, dest: ChainLocation) {
    transferReserveAsset(MultiAsset.from(asset.relativeToLocal(), amount).intoMultiAssets(), dest)
}

fun XcmBuilder.buyExecution(asset: AbsoluteMultiLocation, amount: Balance, weightLimit: WeightLimit) {
    buyExecution(MultiAsset.from(asset.relativeToLocal(), amount), weightLimit)
}

fun XcmBuilder.depositAllAssetsTo(beneficiary: AccountId) {
    depositAsset(All, beneficiary)
}

fun XcmBuilder.payFeesIn(assetId: AssetLocation) {
    payFees(PayFeesMode.Measured(assetId))
}

fun XcmBuilder.payFees(assetId: MultiAssetId, exactFees: Balance) {
    payFees(PayFeesMode.Exact(assetId.withAmount(exactFees)))
}
