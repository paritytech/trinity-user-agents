package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec

import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.CommunicationSessionId
import io.paritytech.polkadotapp.feature_statement_store_impl.data.toEncryptedStatementData
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent

/**
 * Builds plain Request/Response bodies encrypted with pairwise [CommunicationEncryption].
 */
class SingleRequestOutgoingBodyBuilder(
    private val outgoingTopic: CommunicationSessionId,
    private val encryption: CommunicationEncryption,
    private val channelCreator: StatementChannelCreator,
) : OutgoingBodyBuilder {
    override suspend fun buildRequestBody(request: StatementTransportEvent.Request): Statement.Body {
        val encryptedData = with(encryption) { request.toEncryptedStatementData() }
        return Statement.Body(
            channel = channelCreator.requestChannel(outgoingTopic),
            expiry = request.expiry,
            topic1 = outgoingTopic,
            data = encryptedData,
        )
    }

    override suspend fun buildResponseBody(response: StatementTransportEvent.Response): Statement.Body {
        val encryptedData = with(encryption) { response.toEncryptedStatementData() }
        return Statement.Body(
            channel = channelCreator.responseChannel(outgoingTopic),
            expiry = response.expiry,
            topic1 = outgoingTopic,
            data = encryptedData,
        )
    }
}
