package io.paritytech.polkadotapp.feature_xcm_api.versions

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.AsRawScaleValue
import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.paritytech.polkadotapp.common.data.substrate.castToDictEnum
import io.paritytech.polkadotapp.common.utils.scale.ToDynamicScaleInstance
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.RelativeMultiLocation

data class VersionedXcm<T>(
    val xcm: T,
    val version: XcmVersion
)

@JvmName("toEncodableInstanceVersioned")
fun VersionedXcm<out VersionedToDynamicScaleInstance>.toEncodableInstance(): DictEnum.Entry<*> {
    return DictEnum.Entry(
        name = version.enumerationKey(),
        value = xcm.toEncodableInstance(version)
    )
}

fun VersionedXcm<out ToDynamicScaleInstance>.toEncodableInstance(): DictEnum.Entry<*> {
    return DictEnum.Entry(
        name = version.enumerationKey(),
        value = xcm.toEncodableInstance()
    )
}

@JvmName("toEncodableInstanceRaw")
fun VersionedXcm<AsRawScaleValue>.toEncodableInstance(): DictEnum.Entry<*> {
    return DictEnum.Entry(
        name = version.enumerationKey(),
        value = xcm.value
    )
}

fun <T> bindVersionedXcm(instance: Any?, inner: (Any?, xcmVersion: XcmVersion) -> T): VersionedXcm<T> {
    val versionEnum = instance.castToDictEnum()
    val xcmVersion = XcmVersion.fromEnumerationKey(versionEnum.name)

    return VersionedXcm(inner(versionEnum.value, xcmVersion), xcmVersion)
}

fun <T> T.versionedXcm(xcmVersion: XcmVersion): VersionedXcm<T> {
    return VersionedXcm(this, xcmVersion)
}

typealias VersionedXcmLocation = VersionedXcm<RelativeMultiLocation>
