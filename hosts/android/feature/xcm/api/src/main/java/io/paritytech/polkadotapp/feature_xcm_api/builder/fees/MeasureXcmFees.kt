package io.paritytech.polkadotapp.feature_xcm_api.builder.fees

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_xcm_api.builder.XcmBuilder
import io.paritytech.polkadotapp.feature_xcm_api.message.VersionedXcmMessage
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.AssetLocation
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.ChainLocation

/**
 * Measure fees for a given xcm message. Used by [XcmBuilder] when processing [XcmBuilder.payFees]
 * with [PayFeesMode.Measured] specified
 */
interface MeasureXcmFees {
    suspend fun measureFees(
        message: VersionedXcmMessage,
        feeAsset: AssetLocation,
        chainLocation: ChainLocation,
    ): Balance
}
