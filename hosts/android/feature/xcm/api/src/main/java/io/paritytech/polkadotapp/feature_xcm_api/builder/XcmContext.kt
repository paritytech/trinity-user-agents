package io.paritytech.polkadotapp.feature_xcm_api.builder

import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.AbsoluteMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.ChainLocation
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.RelativeMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion

interface XcmContext {
    val xcmVersion: XcmVersion

    val currentLocation: ChainLocation
}

fun XcmContext.localViewOf(location: AbsoluteMultiLocation): RelativeMultiLocation {
    return location.fromPointOfViewOf(currentLocation.location)
}

context(xcmContext: XcmContext)
fun AbsoluteMultiLocation.relativeToLocal(): RelativeMultiLocation {
    return xcmContext.localViewOf(this)
}
