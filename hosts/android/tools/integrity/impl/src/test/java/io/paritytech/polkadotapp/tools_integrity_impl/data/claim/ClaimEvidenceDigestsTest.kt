package io.paritytech.polkadotapp.tools_integrity_impl.data.claim

import io.novasama.substrate_sdk_android.extensions.toHexString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class ClaimEvidenceDigestsTest {
    private val challenge = ByteArray(32) { 0x11 }
    private val candidate = ByteArray(32) { 0x22 }
    private val rawWidevineId = ByteArray(16) { 0x33 }

    // Wire-contract golden values.
    @Test
    fun `device id matches the golden bytes`() {
        assertEquals(
            "55abccdb97c6ca09ba373a5eb19d3f44f3f7aaa06b95deefc3cb14f5eaa879fc",
            ClaimEvidenceDigests.deviceId(rawWidevineId).toHexString()
        )
    }

    @Test
    fun `attestation challenge matches the golden bytes`() {
        val deviceId = ClaimEvidenceDigests.deviceId(rawWidevineId)

        assertEquals(
            "0d408dc2e53e935a844b1ac7d1f34886ce8e91f253ead6d32e29d4608e1ff292",
            ClaimEvidenceDigests.attestationChallenge(challenge, candidate, deviceId).toHexString()
        )
    }

    @Test
    fun `attestation challenge rejects wrong field sizes`() {
        val deviceId = ByteArray(32)

        assertThrows(IllegalArgumentException::class.java) {
            ClaimEvidenceDigests.attestationChallenge(ByteArray(31), candidate, deviceId)
        }
        assertThrows(IllegalArgumentException::class.java) {
            ClaimEvidenceDigests.attestationChallenge(challenge, ByteArray(33), deviceId)
        }
        assertThrows(IllegalArgumentException::class.java) {
            ClaimEvidenceDigests.attestationChallenge(challenge, candidate, ByteArray(16))
        }
    }
}
