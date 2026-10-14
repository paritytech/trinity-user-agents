package io.paritytech.polkadotapp.tools_integrity_impl.data.claim

import io.paritytech.polkadotapp.common.utils.sha256

// Frozen wire derivations shared with the backend.
internal object ClaimEvidenceDigests {
    private const val DOMAIN = "dub/poud/android/v1"
    private const val DEVICE_ID_DOMAIN = "dub/poud/widevine-id/v1"

    fun deviceId(rawWidevineId: ByteArray): ByteArray =
        (DEVICE_ID_DOMAIN.encodeToByteArray() + rawWidevineId).sha256()

    fun attestationChallenge(challenge: ByteArray, candidate: ByteArray, deviceId: ByteArray): ByteArray {
        require(challenge.size == 32) { "challenge must be 32 bytes, got ${challenge.size}" }
        require(candidate.size == 32) { "candidate must be 32 bytes, got ${candidate.size}" }
        require(deviceId.size == 32) { "deviceId must be 32 bytes, got ${deviceId.size}" }
        return (DOMAIN.encodeToByteArray() + challenge + candidate + deviceId).sha256()
    }
}
