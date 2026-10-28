package io.paritytech.polkadotapp.sr25519_vrf

import io.novasama.substrate_sdk_android.encrypt.Sr25519
import org.junit.Assert.assertEquals
import org.junit.Test

class Sr25519VrfCryptoTest {
    @Test
    fun splitsNativeOutputIntoSignatureFields() {
        val keypair = Sr25519.keypairFromSeed(ByteArray(32) { 1 })
        val label = "native-test".encodeToByteArray()
        val items = listOf(Sr25519VrfTranscriptItem("message".encodeToByteArray(), byteArrayOf(7)))

        val signature = Sr25519VrfSigner.sign(keypair, label, items).getOrThrow()

        assertEquals(32 to 64, signature.preOutput.size to signature.proof.size)
    }

    @Test
    fun rejectsBadKeypairLengthBeforeCallingNative() {
        assertEquals(
            Sr25519VrfError.InvalidKeypairLength,
            Sr25519VrfSigner.sign(ByteArray(10), byteArrayOf(), emptyList()).exceptionOrNull(),
        )
    }
}
