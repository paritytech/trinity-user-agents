package io.paritytech.polkadotapp.sr25519_vrf

import io.paritytech.polkadotapp.common.utils.mapError

/** One `append_message(label, value)` call against the merlin transcript. */
class Sr25519VrfTranscriptItem(val label: ByteArray, val value: ByteArray)

/** The sr25519 VRF output: the 32-byte pre-output and the 64-byte DLEQ proof. */
class Sr25519VrfSignature(val preOutput: ByteArray, val proof: ByteArray)

sealed class Sr25519VrfError(message: String) : Exception(message) {
    object InvalidKeypairLength : Sr25519VrfError("sr25519 VRF keypair must be $KEYPAIR_SIZE bytes")
    class SigningFailed(cause: Throwable) : Sr25519VrfError("sr25519 VRF signing failed: ${cause.message}")
}

/**
 * Kotlin boundary over the native [Sr25519VrfCrypto]. Validates the keypair length and turns the
 * native call (which throws on failure) into a [Result].
 *
 * The native side is a pure transcript replayer — `Transcript::new(transcriptLabel)` followed by one
 * `append_message` per item, in order — so callers own the transcript shape their consuming runtime
 * expects.
 */
object Sr25519VrfSigner {
    /**
     * @param keypair 96 bytes — sr25519 key as rawSecretKey(64) ++ rawPublicKey(32).
     * @param transcriptLabel the merlin root domain-separation label.
     * @param items replayed in order as `append_message(item.label, item.value)`.
     */
    fun sign(
        keypair: ByteArray,
        transcriptLabel: ByteArray,
        items: List<Sr25519VrfTranscriptItem>,
    ): Result<Sr25519VrfSignature> {
        if (keypair.size != KEYPAIR_SIZE) return Result.failure(Sr25519VrfError.InvalidKeypairLength)

        return runCatching {
            Sr25519VrfCrypto.sign(
                keypair,
                transcriptLabel,
                items.map(Sr25519VrfTranscriptItem::label).toTypedArray(),
                items.map(Sr25519VrfTranscriptItem::value).toTypedArray(),
            )
        }
            .mapCatching { output ->
                Sr25519VrfSignature(
                    preOutput = output.copyOfRange(0, PRE_OUTPUT_SIZE),
                    proof = output.copyOfRange(PRE_OUTPUT_SIZE, OUTPUT_SIZE),
                )
            }
            .mapError { Sr25519VrfError.SigningFailed(it) }
    }
}

private const val KEYPAIR_SIZE = 96
private const val PRE_OUTPUT_SIZE = 32
private const val OUTPUT_SIZE = 96
