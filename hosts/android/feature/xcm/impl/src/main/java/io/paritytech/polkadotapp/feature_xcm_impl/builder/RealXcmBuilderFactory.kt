package io.paritytech.polkadotapp.feature_xcm_impl.builder

import io.paritytech.polkadotapp.feature_xcm_api.builder.XcmBuilder
import io.paritytech.polkadotapp.feature_xcm_api.builder.fees.MeasureXcmFees
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.ChainLocation
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion
import javax.inject.Inject

class RealXcmBuilderFactory @Inject constructor() : XcmBuilder.Factory {
    override fun create(
        initial: ChainLocation,
        xcmVersion: XcmVersion,
        measureXcmFees: MeasureXcmFees
    ): XcmBuilder {
        return RealXcmBuilder(initial, xcmVersion, measureXcmFees)
    }
}
