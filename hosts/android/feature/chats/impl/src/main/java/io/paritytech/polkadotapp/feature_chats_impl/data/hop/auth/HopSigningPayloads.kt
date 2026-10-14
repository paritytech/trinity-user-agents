package io.paritytech.polkadotapp.feature_chats_impl.data.hop.auth

import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.common.utils.toLittleEndianBytes

/**
 * Domain-separated 32-byte payloads that the HOP node verifies for submit/claim/ack.
 * Byte layouts must remain identical to `substrate/client/hop/src/types.rs` —
 * `signing_payload` and `submit_signing_payload`.
 */
object HopSigningPayloads {
    private val SUBMIT_CONTEXT = "hop-submit-v1:".toByteArray(Charsets.US_ASCII)
    private val CLAIM_CONTEXT = "hop-claim-v1:".toByteArray(Charsets.US_ASCII)
    private val ACK_CONTEXT = "hop-ack-v1:".toByteArray(Charsets.US_ASCII)

    fun submit(data: ByteArray, submitTimestampMs: Long): ByteArray {
        val dataHash = data.blake2b256()
        val timestampBytes = submitTimestampMs.toLittleEndianBytes()

        return (SUBMIT_CONTEXT + dataHash + timestampBytes).blake2b256()
    }

    fun claim(hash: ByteArray): ByteArray = (CLAIM_CONTEXT + hash).blake2b256()

    fun ack(hash: ByteArray): ByteArray = (ACK_CONTEXT + hash).blake2b256()
}
