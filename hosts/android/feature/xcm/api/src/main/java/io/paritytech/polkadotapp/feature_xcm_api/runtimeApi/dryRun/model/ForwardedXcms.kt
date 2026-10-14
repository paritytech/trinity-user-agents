package io.paritytech.polkadotapp.feature_xcm_api.runtimeApi.dryRun.model

import io.paritytech.polkadotapp.chains.network.binding.bindList
import io.paritytech.polkadotapp.common.data.substrate.castToList
import io.paritytech.polkadotapp.feature_xcm_api.message.VersionedRawXcmMessage
import io.paritytech.polkadotapp.feature_xcm_api.message.bindVersionedRawXcmMessage
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.bindVersionedMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.versions.VersionedXcmLocation

typealias ForwardedXcms = List<Pair<VersionedXcmLocation, List<VersionedRawXcmMessage>>>

fun bindForwardedXcms(decodedInstance: Any?): ForwardedXcms {
    return bindList(decodedInstance) { inner ->
        val (locationRaw, messagesRaw) = inner.castToList()
        val messages = bindList(messagesRaw, ::bindVersionedRawXcmMessage)
        val location = bindVersionedMultiLocation(locationRaw)

        location to messages
    }
}
fun ForwardedXcms.getByLocation(location: VersionedXcmLocation): List<VersionedRawXcmMessage> {
    return find { it.first == location }?.second.orEmpty()
}
