package io.paritytech.polkadotapp.tools_integrity_api.claim

interface ClaimDeviceEvidenceProvider {
    /** Collects fresh evidence. `null` means not applicable; failure aborts the claim. */
    suspend fun collectEvidence(): Result<ClaimDeviceEvidence?>
}

/** Base64 wire fields of `POST /api/v1/usernames`, sent together or not at all. */
class ClaimDeviceEvidence(
    val attestationChain: List<String>,
    val deviceChallenge: String,
    val deviceId: String
)
