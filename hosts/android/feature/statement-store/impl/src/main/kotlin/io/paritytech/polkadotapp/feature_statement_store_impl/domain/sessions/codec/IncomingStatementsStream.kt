package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec

import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.CommunicationSessionId
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent
import kotlinx.coroutines.flow.Flow

interface IncomingStatementsStream {
    suspend fun fetch(): Result<List<StatementTransportEvent>>

    fun subscribe(): Flow<Result<List<StatementTransportEvent>>>
}

/**
 * Topic to listen on + the per-peer-device [CommunicationEncryption] used to decrypt
 * statements arriving on that topic.
 */
class IncomingTopicSpec(
    val topic: CommunicationSessionId,
    val senderEncryptionPublicKey: X25519PublicKey,
    val encryption: CommunicationEncryption,
)
