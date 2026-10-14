package io.paritytech.polkadotapp.feature_xcm_api.builder.fees

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_xcm_api.message.VersionedXcmMessage
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.AssetLocation
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.ChainLocation

class UnsupportedMeasureXcmFees : MeasureXcmFees {
    override suspend fun measureFees(
        message: VersionedXcmMessage,
        feeAsset: AssetLocation,
        chainLocation: ChainLocation
    ): Balance {
        error("Measurement not supported")
    }
}
