package io.paritytech.polkadotapp.tools_assethub_sdk_impl.fee

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.feature_transactions.api.data.extensions.ChargeAssetTxPayment.Companion.chargeAssetTxPayment
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.CustomAssetFeePayment
import io.paritytech.polkadotapp.feature_transactions.api.data.fee.SimpleFee
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.Fee
import io.paritytech.polkadotapp.feature_xcm_api.converter.asset.ChainAssetLocationConverter
import io.paritytech.polkadotapp.feature_xcm_api.converter.asset.encodableMultiLocationOf
import io.paritytech.polkadotapp.feature_xcm_api.versions.detector.XcmVersionDetector
import io.paritytech.polkadotapp.tools_assethub_sdk_api.swap.model.SwapDirection
import io.paritytech.polkadotapp.tools_assethub_sdk_impl.data.AssetConversionQuoter
import io.paritytech.polkadotapp.tools_assethub_sdk_impl.data.detectAssetIdXcmVersion

class AssetConversionFeePayment @AssistedInject constructor(
    @Assisted feePaymentAsset: Chain.Asset,
    @Assisted private val quoter: AssetConversionQuoter,
    @Assisted private val multiLocationConverter: ChainAssetLocationConverter,
    private val xcmVersionDetector: XcmVersionDetector,
) : CustomAssetFeePayment(feePaymentAsset) {
    override suspend fun modifyExtrinsicChecked(
        chainAsset: Chain.Asset,
        extrinsicBuilder: ExtrinsicBuilder
    ) {
        val xcmVersion = xcmVersionDetector.detectAssetIdXcmVersion(chainAsset.chainId, extrinsicBuilder.runtime)
        val assetId = multiLocationConverter.encodableMultiLocationOf(chainAsset, xcmVersion)
        extrinsicBuilder.chargeAssetTxPayment(assetId)
    }

    override suspend fun convertNativeFeeChecked(
        chainAsset: Chain.Asset,
        nativeFee: Fee
    ): Fee {
        val quote = quoter.quote(
            fromAsset = chainAsset,
            toAsset = nativeFee.asset,
            amount = nativeFee.amount,
            direction = SwapDirection.SPECIFIED_OUT
        )

        return SimpleFee(quote, chainAsset)
    }

    @AssistedFactory
    interface Factory {
        fun create(
            feePaymentAsset: Chain.Asset,
            quoter: AssetConversionQuoter,
            multiLocationConverter: ChainAssetLocationConverter
        ): AssetConversionFeePayment
    }
}
