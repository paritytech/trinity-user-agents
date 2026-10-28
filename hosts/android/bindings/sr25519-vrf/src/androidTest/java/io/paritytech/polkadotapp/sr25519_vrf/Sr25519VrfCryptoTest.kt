package io.paritytech.polkadotapp.sr25519_vrf

import android.util.Log
import io.novasama.substrate_sdk_android.encrypt.Sr25519
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class Sr25519VrfCryptoTest {

    @OptIn(ExperimentalStdlibApi::class)
    @Test
    fun signsAndReturnsStableNinetySixByteOutput() {
        val keypair = Sr25519.keypairFromSeed(ByteArray(32) { 1 })
        val items = airdropItems(gameIndex = 7, publicKey = keypair.copyOfRange(64, 96))

        val first = Sr25519VrfSigner.sign(keypair, AIRDROP_LABEL, items).getOrThrow()
        val second = Sr25519VrfSigner.sign(keypair, AIRDROP_LABEL, items).getOrThrow()

        assertEquals(32, first.preOutput.size)
        assertEquals(64, first.proof.size)
        // The VRF pre-output is deterministic for a given key + transcript; the proof carries fresh
        // witness randomness and may differ between calls.
        assertArrayEquals(first.preOutput, second.preOutput)

        Log.d("Sr25519VrfCryptoTest", "preOutput=${first.preOutput.toHexString()}")
    }

    @Test
    fun rejectsBadKeypairLength() {
        assertTrue(Sr25519VrfSigner.sign(ByteArray(10), AIRDROP_LABEL, emptyList()).isFailure)
    }

    @Test
    fun signsWithoutItems() {
        val keypair = Sr25519.keypairFromSeed(ByteArray(32) { 1 })

        val signature = Sr25519VrfSigner.sign(keypair, AIRDROP_LABEL, emptyList()).getOrThrow()

        assertEquals(32, signature.preOutput.size)
        assertEquals(64, signature.proof.size)
    }

    private fun airdropItems(gameIndex: Int, publicKey: ByteArray): List<Sr25519VrfTranscriptItem> {
        return listOf(
            Sr25519VrfTranscriptItem(
                label = "domain".toByteArray(Charsets.US_ASCII),
                value = AIRDROP_LABEL + eventId(gameIndex),
            ),
            Sr25519VrfTranscriptItem(label = "signer".toByteArray(Charsets.US_ASCII), value = publicKey),
        )
    }

    // "pop:game:airdrop:" + 11 spaces + game_index.to_be_bytes() = 32 bytes.
    private fun eventId(gameIndex: Int): ByteArray =
        "pop:game:airdrop:           ".toByteArray(Charsets.US_ASCII) +
            byteArrayOf(
                (gameIndex ushr 24).toByte(),
                (gameIndex ushr 16).toByte(),
                (gameIndex ushr 8).toByte(),
                gameIndex.toByte(),
            )
}

private val AIRDROP_LABEL = "pop:airdrop".toByteArray(Charsets.US_ASCII)
