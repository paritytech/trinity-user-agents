package io.paritytech.polkadotapp.feature_chats_impl.data.hop

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.novasama.substrate_sdk_android.wsrpc.executeAsync
import io.novasama.substrate_sdk_android.wsrpc.mappers.nonNull
import io.novasama.substrate_sdk_android.wsrpc.mappers.pojo
import io.novasama.substrate_sdk_android.wsrpc.request.DeliveryType
import io.paritytech.polkadotapp.chains.util.sign
import io.paritytech.polkadotapp.chains.util.signing.MessageSigningContext
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.auth.HopSigner
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.auth.HopSigningPayloads
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.download.hopBitswapCid
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.encryption.HopEncryption
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.encryption.HopTicketKeyDerivation
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.BitswapGetRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.BitswapOutcome
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.ClaimOutcome
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopAckRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopClaimRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopMultiSignature
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopMultiSigner
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopSubmitRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.model.HopSubmitResult
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.RetryableTransferException
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TerminalTransferException
import io.paritytech.polkadotapp.tools_ipfs_api.Cids
import kotlinx.serialization.encodeToByteArray
import kotlinx.serialization.encodeToHexString
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Provider

class HopService @Inject constructor(
    private val socketServiceProvider: Provider<SocketService>,
    private val hopSigner: HopSigner,
    private val ticketKeyDerivation: HopTicketKeyDerivation
) {
    class FetchedEntry(
        val bytes: ByteArray,
        private val ack: (suspend () -> Unit)? = null
    ) {
        suspend fun markPersisted() = ack?.invoke()
    }

    suspend fun <R> withSession(nodeUrl: String, block: suspend Session.() -> R): R {
        Timber.d("Starting Hop session with node $nodeUrl")
        val socketService = socketServiceProvider.get()
        socketService.start(nodeUrl, remainPaused = false)
        try {
            return block(Session(socketService))
        } finally {
            socketService.stop()
        }
    }

    inner class Session(private val socketService: SocketService) {
        suspend fun submitEntry(plaintext: ByteArray, ticket: HopTicket): ByteArray {
            val keys = ticket.createKeys()
            val encrypted = keys.encryption.encrypt(plaintext)

            val timestamp = System.currentTimeMillis()
            val payload = HopSigningPayloads.submit(encrypted, timestamp)
            val signature = hopSigner.sign(payload)

            submit(
                data = encrypted,
                recipients = listOf(keys.recipient),
                signature = signature,
                signer = hopSigner.multiSigner(),
                submitTimestampMs = timestamp
            )

            return encrypted.blake2b256()
        }

        suspend fun fetchEntry(hash: ByteArray, ticket: HopTicket, fallbackNodes: List<String>): FetchedEntry? {
            val keys = ticket.createKeys()

            return when (val claimOutcome = claimEntryCatching(hash, keys)) {
                is ClaimOutcome.Found -> FetchedEntry(
                    bytes = keys.encryption.decrypt(claimOutcome.data),
                    ack = { ackEntry(hash, keys) }
                )

                is ClaimOutcome.NotFound -> sweepFallbackNodes(hash, keys, fallbackNodes)
                is ClaimOutcome.Retryable -> throw RetryableTransferException(
                    "hop_claim failed for ${hash.toHexString(withPrefix = true)}: " +
                        "code=${claimOutcome.code}, ${claimOutcome.message}"
                )
            }
        }

        private suspend fun claimEntryCatching(hash: ByteArray, keys: TicketKeys): ClaimOutcome {
            return runCancellableCatching { claimEntry(hash, keys) }
                .getOrElse { error ->
                    Timber.d(error, "hop_claim threw for ${hash.toHexString(withPrefix = true)}")
                    ClaimOutcome.Retryable(code = CLAIM_TRANSPORT_FAILURE_CODE, message = error.message.orEmpty())
                }
        }

        private suspend fun ackEntry(hash: ByteArray, keys: TicketKeys) {
            val ackSignature = HopMultiSignature.SR25519(
                keys.signingKeyPair.sign(HopSigningPayloads.ack(hash), MessageSigningContext.trustedContent())
            )
            runCancellableCatching { ack(hash, ackSignature) }
                .onFailure { Timber.w(it, "hop_ack failed for ${hash.toHexString(withPrefix = true)}; treating as terminal") }
        }

        private suspend fun claimEntry(hash: ByteArray, keys: TicketKeys): ClaimOutcome {
            val claimSignature = HopMultiSignature.SR25519(
                keys.signingKeyPair.sign(HopSigningPayloads.claim(hash), MessageSigningContext.trustedContent())
            )
            return claim(hash, claimSignature)
        }

        private suspend fun sweepFallbackNodes(hash: ByteArray, keys: TicketKeys, nodes: List<String>): FetchedEntry? {
            var sawRetryable = false

            for (node in nodes) {
                when (val outcome = fetchFromNode(hash, node)) {
                    is BitswapOutcome.Found -> return FetchedEntry(keys.encryption.decrypt(outcome.data))
                    is BitswapOutcome.NotFound -> Unit
                    is BitswapOutcome.Retryable -> sawRetryable = true
                    is BitswapOutcome.InvalidCid -> throw TerminalTransferException(
                        "bitswap reported InvalidCid for ${hash.toHexString(withPrefix = true)}"
                    )
                }
            }

            if (sawRetryable) {
                throw RetryableTransferException(
                    "Entry ${hash.toHexString(withPrefix = true)} not available yet; some fallback nodes were retryable"
                )
            }

            return null
        }

        private suspend fun fetchFromNode(hash: ByteArray, node: String): BitswapOutcome {
            return runCancellableCatching { withSession(node) { verifiedBitswapGet(hash) } }
                .getOrElse { error ->
                    Timber.d(error, "bitswap fetch threw for ${hash.toHexString(withPrefix = true)} on $node")
                    BitswapOutcome.Retryable(code = BITSWAP_TRANSPORT_FAILURE_CODE, message = error.message.orEmpty())
                }
        }

        private suspend fun verifiedBitswapGet(hash: ByteArray): BitswapOutcome {
            return when (val outcome = bitswapGet(Cids.hopBitswapCid(hash))) {
                is BitswapOutcome.Found -> verify(outcome, hash)
                else -> outcome
            }
        }

        private fun verify(found: BitswapOutcome.Found, hash: ByteArray): BitswapOutcome {
            if (found.data.blake2b256().contentEquals(hash)) return found

            Timber.w("bitswap bytes for ${hash.toHexString(withPrefix = true)} failed blake2b check; discarding")
            return BitswapOutcome.NotFound
        }

        private suspend fun submit(
            data: ByteArray,
            recipients: List<HopMultiSigner>,
            signature: HopMultiSignature,
            signer: HopMultiSigner,
            submitTimestampMs: Long
        ): HopSubmitResult {
            val encodedRecipients = recipients.map { BinaryScale.encodeToHexString(it) }
            val encodedSignature = BinaryScale.encodeToByteArray(signature)
            val encodedSigner = BinaryScale.encodeToByteArray(signer)

            val request = HopSubmitRequest(
                data = data.toHexString(withPrefix = true),
                recipients = encodedRecipients,
                signature = encodedSignature.toHexString(withPrefix = true),
                signer = encodedSigner.toHexString(withPrefix = true),
                submitTimestamp = submitTimestampMs
            )

            return socketService.executeAsync(
                request = request,
                deliveryType = DeliveryType.AT_MOST_ONCE,
                mapper = pojo<HopSubmitResult>().nonNull()
            )
        }

        private suspend fun claim(
            hash: ByteArray,
            signature: HopMultiSignature
        ): ClaimOutcome {
            val encodedSignature = BinaryScale.encodeToByteArray(signature)

            val request = HopClaimRequest(
                hash = hash.toHexString(withPrefix = true),
                signature = encodedSignature.toHexString(withPrefix = true)
            )

            val response = socketService.executeAsync(
                request = request,
                deliveryType = DeliveryType.AT_MOST_ONCE
            )

            response.error?.let {
                Timber.d("hop_claim error (code=${it.code}, message=${it.message})")

                return when (it.code) {
                    CLAIM_NOT_FOUND_CODE -> ClaimOutcome.NotFound
                    else -> ClaimOutcome.Retryable(it.code, it.message)
                }
            }

            val hex = response.result as? String ?: return ClaimOutcome.NotFound
            return ClaimOutcome.Found(hex.fromHex())
        }

        private suspend fun ack(
            hash: ByteArray,
            signature: HopMultiSignature
        ) {
            val encodedSignature = BinaryScale.encodeToByteArray(signature)

            val request = HopAckRequest(
                hash = hash.toHexString(withPrefix = true),
                signature = encodedSignature.toHexString(withPrefix = true)
            )

            val response = socketService.executeAsync(
                request = request,
                deliveryType = DeliveryType.AT_MOST_ONCE
            )

            response.error?.let {
                error("hop_ack failed. Code: ${it.code}. Message: ${it.message}")
            }
        }

        private suspend fun bitswapGet(cid: String): BitswapOutcome {
            val response = socketService.executeAsync(
                request = BitswapGetRequest(cid),
                deliveryType = DeliveryType.AT_MOST_ONCE
            )

            response.error?.let {
                return when (it.code) {
                    BITSWAP_INVALID_CID_CODE -> BitswapOutcome.InvalidCid
                    BITSWAP_NOT_FOUND_CODE -> BitswapOutcome.NotFound
                    else -> BitswapOutcome.Retryable(it.code, it.message)
                }
            }

            val hex = response.result as? String
                ?: return BitswapOutcome.Retryable(BITSWAP_EMPTY_RESULT_CODE, "bitswap_v1_get returned no result")
            return BitswapOutcome.Found(hex.fromHex())
        }
    }

    private fun HopTicket.createKeys(): TicketKeys {
        val signingKeyPair = ticketKeyDerivation.deriveSigningKeyPair(this)

        return TicketKeys(
            encryption = HopEncryption(ticketKeyDerivation.deriveEncryptionKey(this)),
            signingKeyPair = signingKeyPair,
            recipient = HopMultiSigner.SR25519(signingKeyPair.publicKey)
        )
    }

    private class TicketKeys(
        val encryption: HopEncryption,
        val signingKeyPair: Sr25519Keypair,
        val recipient: HopMultiSigner
    )

    companion object {
        const val CHUNK_SIZE_BYTES = 2_000_000
        const val ENTRY_OVERHEAD_BYTES = 64
        const val INLINE_MAX_BYTES = CHUNK_SIZE_BYTES - ENTRY_OVERHEAD_BYTES
    }
}

private const val CLAIM_NOT_FOUND_CODE = 1004
private const val CLAIM_TRANSPORT_FAILURE_CODE = -1
private const val BITSWAP_INVALID_CID_CODE = -32602
private const val BITSWAP_NOT_FOUND_CODE = -32810
private const val BITSWAP_EMPTY_RESULT_CODE = 0
private const val BITSWAP_TRANSPORT_FAILURE_CODE = -1
