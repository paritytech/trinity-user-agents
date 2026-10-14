package io.paritytech.polkadotapp.feature_xcm_api.asset

import io.paritytech.polkadotapp.common.data.substrate.castToDictEnum
import io.paritytech.polkadotapp.common.data.substrate.castToStruct
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.bindMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.versions.VersionedXcm
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion
import io.paritytech.polkadotapp.feature_xcm_api.versions.bindVersionedXcm

fun bindVersionedLocatableMultiAsset(decoded: Any?): VersionedXcm<LocatableMultiAsset> {
    return bindVersionedXcm(decoded, ::bindLocatableMultiAsset)
}

fun bindLocatableMultiAsset(decoded: Any?, xcmVersion: XcmVersion): LocatableMultiAsset {
    val asStruct = decoded.castToStruct()

    return LocatableMultiAsset(
        location = bindMultiLocation(asStruct["location"]),
        assetId = bindMultiAssetId(asStruct["asset_id"], xcmVersion)
    )
}

fun bindMultiAssetId(decoded: Any?, xcmVersion: XcmVersion): MultiAssetId {
    // V4 removed variants of MultiAssetId, leaving only flattened value of Concrete
    val locationInstance = if (xcmVersion >= XcmVersion.V4) {
        decoded
    } else {
        extractConcreteLocation(decoded)
    }

    return MultiAssetId(bindMultiLocation(locationInstance))
}

private fun extractConcreteLocation(decoded: Any?): Any? {
    val variant = decoded.castToDictEnum()
    require(variant.name == "Concrete") {
        "Asset ids besides Concrete are not supported"
    }

    return variant.value
}
