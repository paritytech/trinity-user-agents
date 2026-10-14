package io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestDecrypted
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestEncryptedRemote
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Handles encryption and decryption of chat requests.
 */
interface ChatRequestCrypto {
    /**
     * Encrypts a decrypted chat request with one-time encryption key
     *
     * @param decrypted The decrypted request to encrypt
     * @param peerPublicKey The peer's public key for encryption
     * @return The encrypted request along with the local public key and shared secret
     */
    suspend fun encrypt(
        decrypted: ChatRequestDecrypted,
        peerPublicKey: X25519PublicKey
    ): Result<ChatRequestEncryptedRemote>

    /**
     * Decrypts an encrypted chat request received from a peer.
     *
     * @param encrypted The encrypted request received
     * @param domain The shared secret derivation domain (e.g., CHAT or CANDIDATE)
     * @return The decrypted request
     */
    suspend fun decrypt(
        encrypted: ChatRequestEncryptedRemote,
        domain: SharedSecretDerivationDomain
    ): Result<ChatRequestDecrypted>
}

@Singleton
class RealChatRequestCrypto @Inject constructor(
    private val encryptionFactory: CommunicationEncryption.Factory
) : ChatRequestCrypto {
    override suspend fun encrypt(
        decrypted: ChatRequestDecrypted,
        peerPublicKey: X25519PublicKey
    ): Result<ChatRequestEncryptedRemote> = runCatching {
        val decryptedBytes = BinaryScale.encodeToByteArray(decrypted)

        val encryption = encryptionFactory.createOneTimeUse(peerPublicKey)
        val encryptedBytes = encryption.encrypt(decryptedBytes)

        ChatRequestEncryptedRemote(
            encryptionPubKey = encryption.localPublicKey.bytes.value,
            encryptedRequest = encryptedBytes
        )
    }

    override suspend fun decrypt(
        encrypted: ChatRequestEncryptedRemote,
        domain: SharedSecretDerivationDomain
    ): Result<ChatRequestDecrypted> = runCatching {
        val oneTimePeerKey = encrypted.encryptionPubKey.requireX25519PublicKey()
        val encryption = encryptionFactory.create(domain, oneTimePeerKey)
        val decryptedBytes = encryption.decrypt(encrypted.encryptedRequest)

        BinaryScale.decodeFromByteArray<ChatRequestDecrypted>(decryptedBytes)
    }
}
