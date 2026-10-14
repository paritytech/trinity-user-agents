package io.paritytech.polkadotapp.feature_transactions.api.data.fee

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.Fee

class NativeFeePayment : FeePayment {
    override suspend fun modifyExtrinsic(
        extrinsicBuilder: ExtrinsicBuilder
    ) {}

    override suspend fun convertNativeFee(
        nativeFee: Fee
    ): Fee {
        return nativeFee
    }
}
