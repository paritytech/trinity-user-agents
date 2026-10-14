package io.paritytech.polkadotapp.feature_statement_store_api.domain

import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.CommunicationSessionEvent
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.CommunicationSessionId
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccount
import kotlinx.coroutines.flow.Flow

interface CommunicationSession {
    val localAccount: SessionAccount
    val remoteAccount: SessionAccount

    val incomingSessionId: CommunicationSessionId

    fun subscribeEvents(): Flow<CommunicationSessionEvent>

    fun sendMessage(message: EncodedMessage)

    fun encrypt(data: ByteArray): ByteArray

    fun generateSharedIncomingSessionValue(salt: ByteArray): ByteArray
    fun generateSharedOutgoingSessionValue(salt: ByteArray): ByteArray
}
