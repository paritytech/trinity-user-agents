package io.paritytech.polkadotapp.feature_transactions.api.data.fee

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.util.isUtilityAsset
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.Fee

abstract class CustomAssetFeePayment(
    private val asset: Chain.Asset
) : FeePayment {
    private val nativeDelegate: NativeFeePayment = NativeFeePayment()

    protected abstract suspend fun modifyExtrinsicChecked(
        chainAsset: Chain.Asset,
        extrinsicBuilder: ExtrinsicBuilder
    )

    protected abstract suspend fun convertNativeFeeChecked(
        chainAsset: Chain.Asset,
        nativeFee: Fee
    ): Fee

    final override suspend fun modifyExtrinsic(
        extrinsicBuilder: ExtrinsicBuilder
    ) {
        if (asset.isUtilityAsset) {
            nativeDelegate.modifyExtrinsic(extrinsicBuilder)
        } else {
            modifyExtrinsicChecked(asset, extrinsicBuilder)
        }
    }

    final override suspend fun convertNativeFee(
        nativeFee: Fee
    ): Fee {
        return if (asset.isUtilityAsset) {
            nativeDelegate.convertNativeFee(nativeFee)
        } else {
            convertNativeFeeChecked(asset, nativeFee)
        }
    }
}
