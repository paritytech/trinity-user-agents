package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.CommunicationSessionId
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.ContactDeviceProvider
import io.paritytech.polkadotapp.feature_statement_store_impl.data.encryption.MultiDeviceEnvelopeEncryption
import io.paritytech.polkadotapp.feature_statement_store_impl.data.encryption.toEnvelopeRecipients
import io.paritytech.polkadotapp.feature_statement_store_impl.data.toMultiDeviceEncryptedStatementData
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent

/**
 * Builds MultiDevice Request/Response bodies: wraps the payload in a per-device envelope
 * (via [envelopeEncryption]) and then encrypts the whole thing with the outer pairwise
 * [CommunicationEncryption], mirroring the wire format of the single-device path.
 */
class MultiDeviceOutgoingBodyBuilder(
    private val outgoingTopic: CommunicationSessionId,
    private val encryption: CommunicationEncryption,
    private val envelopeEncryption: MultiDeviceEnvelopeEncryption,
    private val remoteAccountId: AccountId,
    private val contactDeviceProvider: ContactDeviceProvider,
    private val channelCreator: StatementChannelCreator,
) : OutgoingBodyBuilder {
    override suspend fun buildRequestBody(request: StatementTransportEvent.Request): Statement.Body {
        val encryptedData = with(encryption) {
            request.toMultiDeviceEncryptedStatementData(
                envelopeEncryption = envelopeEncryption,
                recipients = currentRecipients(),
            )
        }

        return Statement.Body(
            channel = channelCreator.requestChannel(outgoingTopic),
            expiry = request.expiry,
            topic1 = outgoingTopic,
            data = encryptedData,
        )
    }

    override suspend fun buildResponseBody(response: StatementTransportEvent.Response): Statement.Body {
        val encryptedData = with(encryption) {
            response.toMultiDeviceEncryptedStatementData(
                envelopeEncryption = envelopeEncryption,
                recipients = currentRecipients(),
            )
        }

        return Statement.Body(
            channel = channelCreator.responseChannel(outgoingTopic),
            expiry = response.expiry,
            topic1 = outgoingTopic,
            data = encryptedData,
        )
    }

    private suspend fun currentRecipients(): List<MultiDeviceEnvelopeEncryption.Recipient> {
        return contactDeviceProvider.getDevices(remoteAccountId).toEnvelopeRecipients()
    }
}
