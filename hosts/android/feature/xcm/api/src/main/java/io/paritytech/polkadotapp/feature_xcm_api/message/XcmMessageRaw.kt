package io.paritytech.polkadotapp.feature_xcm_api.message

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.AsRawScaleValue
import io.paritytech.polkadotapp.feature_xcm_api.versions.VersionedXcm
import io.paritytech.polkadotapp.feature_xcm_api.versions.bindVersionedXcm

typealias XcmMessageRaw = AsRawScaleValue
typealias VersionedRawXcmMessage = VersionedXcm<XcmMessageRaw>

fun bindVersionedRawXcmMessage(decodedInstance: Any?): VersionedRawXcmMessage = bindVersionedXcm(decodedInstance) { inner, _ ->
    AsRawScaleValue(inner)
}

fun bindRawXcmMessage(decodedInstance: Any?): XcmMessageRaw = AsRawScaleValue(decodedInstance)
