package io.paritytech.polkadotapp.sr25519_vrf

import org.junit.Assert.assertEquals
import org.junit.Test

class Sr25519VrfCryptoTest {
    @Test
    fun rejectsBadKeypairLengthBeforeCallingNative() {
        assertEquals(
            Sr25519VrfError.InvalidKeypairLength,
            Sr25519VrfSigner.sign(ByteArray(10), byteArrayOf(), emptyList()).exceptionOrNull(),
        )
    }
}
