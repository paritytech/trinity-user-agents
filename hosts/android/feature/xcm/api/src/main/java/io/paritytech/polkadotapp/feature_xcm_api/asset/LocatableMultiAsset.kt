package io.paritytech.polkadotapp.feature_xcm_api.asset

import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.RelativeMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.versions.VersionedXcm

data class LocatableMultiAsset(
    val location: RelativeMultiLocation,
    val assetId: MultiAssetId
)

typealias VersionedLocatableMultiAsset = VersionedXcm<LocatableMultiAsset>
