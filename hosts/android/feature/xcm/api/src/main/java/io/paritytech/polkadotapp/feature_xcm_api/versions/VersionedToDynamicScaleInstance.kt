package io.paritytech.polkadotapp.feature_xcm_api.versions

interface VersionedToDynamicScaleInstance {
    fun toEncodableInstance(xcmVersion: XcmVersion): Any?
}
