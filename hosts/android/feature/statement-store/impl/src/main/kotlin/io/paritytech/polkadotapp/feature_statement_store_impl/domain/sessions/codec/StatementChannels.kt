package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec

import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.CommunicationSessionId
import javax.inject.Inject

/**
 * Derives request/response channel hashes for a topic. Single- and multi-device builders
 * share this so wire-level channel layout stays identical.
 */
class StatementChannelCreator @Inject constructor() {
    fun requestChannel(topic: CommunicationSessionId): ByteArray = CHANNEL_REQUEST.blake2b256(key = topic)

    fun responseChannel(topic: CommunicationSessionId): ByteArray = CHANNEL_RESPONSE.blake2b256(key = topic)

    private companion object {
        private val CHANNEL_REQUEST = "request".toByteArray(Charsets.UTF_8)
        private val CHANNEL_RESPONSE = "response".toByteArray(Charsets.UTF_8)
    }
}
