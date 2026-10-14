package io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest

import io.novasama.substrate_sdk_android.encrypt.EncryptionType
import io.novasama.substrate_sdk_android.encrypt.SignatureVerifier
import io.novasama.substrate_sdk_android.encrypt.SignatureWrapper
import io.novasama.substrate_sdk_android.encrypt.Signer
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.MultiSignature
import io.novasama.substrate_sdk_android.ss58.SS58Encoder.publicKeyToSubstrateAccountId
import io.paritytech.polkadotapp.chains.util.signing.MessageSigningContext
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.data.sign.AccountBytesSigner
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestMessage
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestProofPayload
import io.paritytech.polkadotapp.feature_statement_store_api.data.models.StatementProofRemote
import javax.inject.Inject

/**
 * Handles signing and verification of chat request proofs.
 */
interface ChatRequestProver {
    /**
     * Creates a signed proof for a chat request message.
     *
     * @param message The chat request message to sign
     * @param acceptorAccountId The account ID of the request acceptor
     * @return The signed proof
     */
    suspend fun createProof(
        message: ChatRequestMessage,
        signer: MetaAccount,
        acceptorAccountId: AccountId
    ): Result<StatementProofRemote>

    /**
     * Verifies a chat request proof.
     *
     * @param message The chat request message
     * @param acceptorAccountId The account ID of the request acceptor
     * @param proof The proof to verify
     * @return True if the proof is valid, false otherwise
     */
    suspend fun verifyProof(
        message: ChatRequestMessage,
        acceptorAccountId: AccountId,
        proof: StatementProofRemote
    ): Result<Boolean>
}

internal class RealChatRequestProver @Inject constructor(
    private val accountBytesSigner: AccountBytesSigner,
) : ChatRequestProver {
    override suspend fun createProof(
        message: ChatRequestMessage,
        signer: MetaAccount,
        acceptorAccountId: AccountId
    ): Result<StatementProofRemote> = runCatching {
        val payloadBytes = createProofPayload(message, acceptorAccountId)

        accountBytesSigner.signRawBytes(payloadBytes, MessageSigningContext.trustedContent(), signer)
            .toStatementProofRemote(signer.defaultPubKey())
    }

    override suspend fun verifyProof(
        message: ChatRequestMessage,
        acceptorAccountId: AccountId,
        proof: StatementProofRemote
    ): Result<Boolean> = runCatching {
        SignatureVerifier.verify(
            signatureWrapper = proof.toSignatureWrapper(),
            messageHashing = Signer.MessageHashing.SUBSTRATE,
            data = createProofPayload(message, acceptorAccountId),
            publicKey = proof.signerPubKey().value
        )
    }

    private fun createProofPayload(
        message: ChatRequestMessage,
        acceptorAccountId: AccountId
    ): ByteArray {
        val payload = ChatRequestProofPayload(
            message = message,
            acceptor = acceptorAccountId.value
        )
        return BinaryScale.encodeToByteArray(payload)
    }

    private fun MultiSignature.toStatementProofRemote(signer: EncodedPublicKey): StatementProofRemote {
        return when (encryptionType) {
            EncryptionType.SR25519 -> StatementProofRemote.Sr25519(
                signature = value,
                signer = signer.value
            )

            else -> error("Not supported encryption scheme: $encryptionType")
        }
    }

    private fun StatementProofRemote.toSignatureWrapper(): SignatureWrapper {
        return when (this) {
            is StatementProofRemote.Sr25519 -> SignatureWrapper.Sr25519(signature)
        }
    }
}

fun StatementProofRemote.signerPubKey(): EncodedPublicKey {
    return when (this) {
        is StatementProofRemote.Sr25519 -> signer.toDataByteArray()
    }
}

fun StatementProofRemote.signerAccountId(): AccountId {
    return signerPubKey().value.publicKeyToSubstrateAccountId().toDataByteArray()
}
