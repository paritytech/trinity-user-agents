package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec

import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_impl.data.decryptAndDecodeEvent
import io.paritytech.polkadotapp.feature_statement_store_impl.data.decryptAndDecodeOwnEvent
import io.paritytech.polkadotapp.feature_statement_store_impl.data.encryption.MultiDeviceEnvelopeEncryption
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent
import timber.log.Timber

/**
 * Decodes on-wire [Statement]s into [StatementTransportEvent]s. Handles both plain
 * (Request/Response) and multi-device envelope (MultiRequest/MultiResponse) wire formats
 * transparently.
 */
class StatementDecoder(
    private val encryption: CommunicationEncryption,
    private val envelopeEncryption: MultiDeviceEnvelopeEncryption,
    private val peerDevices: suspend () -> List<MultiDeviceEnvelopeEncryption.Recipient>,
) {
    /** Decodes a statement received from a peer using the per-peer-device receive encryption. */
    suspend fun decode(
        statement: Statement,
        senderEncryptionPublicKey: X25519PublicKey,
        receiveEncryption: CommunicationEncryption,
    ): StatementTransportEvent? = runCatching {
        with(receiveEncryption) {
            statement.decryptAndDecodeEvent(
                envelopeEncryption = envelopeEncryption,
                senderEncryptionPublicKey = senderEncryptionPublicKey,
            )
        }
    }.onFailure {
        Timber.w(it, "decode failed (source=peer, size=${statement.body.data.size})")
    }.getOrNull()

    /** Decodes one of our own previously-submitted statements (no envelope entry for us). */
    suspend fun decodeOur(statement: Statement): StatementTransportEvent? = runCatching {
        with(encryption) {
            statement.decryptAndDecodeOwnEvent(
                envelopeEncryption = envelopeEncryption,
                peerDevices = peerDevices(),
            )
        }
    }.onFailure {
        Timber.w(it, "decode failed (source=self, size=${statement.body.data.size})")
    }.getOrNull()
}
