package io.paritytech.polkadotapp.feature_transfers_api.data.repository

import io.paritytech.polkadotapp.feature_transfers_api.domain.model.SendRecipient

interface SendRecipientRepository {
    suspend fun getSendRecipients(): Result<List<SendRecipient>>

    suspend fun addSendRecipient(recipient: SendRecipient): Result<Unit>
}
