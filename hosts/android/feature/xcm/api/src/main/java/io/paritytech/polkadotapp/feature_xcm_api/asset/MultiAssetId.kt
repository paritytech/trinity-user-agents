package io.paritytech.polkadotapp.feature_xcm_api.asset

import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.RelativeMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.versions.VersionedToDynamicScaleInstance
import io.paritytech.polkadotapp.feature_xcm_api.versions.XcmVersion

@JvmInline
value class MultiAssetId(val multiLocation: RelativeMultiLocation) : VersionedToDynamicScaleInstance {
    override fun toEncodableInstance(xcmVersion: XcmVersion): Any? {
        // V4 removed variants of MultiAssetId, leaving only flattened value of Concrete
        return if (xcmVersion >= XcmVersion.V4) {
            multiLocation.toEncodableInstance(xcmVersion)
        } else {
            DictEnum.Entry(
                name = "Concrete",
                value = multiLocation.toEncodableInstance(xcmVersion)
            )
        }
    }

    override fun toString(): String {
        return multiLocation.toString()
    }
}

fun MultiAssetId.withAmount(amount: Balance): MultiAsset {
    return MultiAsset.from(multiLocation, amount)
}
