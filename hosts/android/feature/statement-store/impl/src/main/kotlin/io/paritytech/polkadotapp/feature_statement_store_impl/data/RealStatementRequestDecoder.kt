package io.paritytech.polkadotapp.feature_statement_store_impl.data

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementRequestDecoder
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.feature_statement_store_impl.data.encryption.MultiDeviceEnvelopeEncryption
import io.paritytech.polkadotapp.feature_statement_store_impl.data.models.scale.StructuredStatementData
import javax.inject.Inject

class RealStatementRequestDecoder @Inject constructor(
    private val encryptionFactory: CommunicationEncryption.Factory,
    private val envelopeEncryptionFactory: MultiDeviceEnvelopeEncryption.Factory,
) : StatementRequestDecoder {
    override suspend fun decodeMessages(
        sharedSecretDerivationDomain: SharedSecretDerivationDomain,
        peerEncryptionPublicKey: X25519PublicKey,
        ourStatementAccountId: AccountId,
        encryptedRequest: ByteArray,
    ): Result<List<EncodedMessage>> = runCatching {
        val encryption = encryptionFactory.create(sharedSecretDerivationDomain, peerEncryptionPublicKey)
        val envelopeEncryption = envelopeEncryptionFactory.create(ourStatementAccountId)

        val single = with(encryption) {
            encryptedRequest.decryptAndDecodeStructuredSingle(
                envelopeEncryption = envelopeEncryption,
                senderEncryptionPublicKey = peerEncryptionPublicKey,
            )
        }

        when (single) {
            is StructuredStatementData.Request -> single.messages
            is StructuredStatementData.Response -> emptyList()
        }
    }
}
