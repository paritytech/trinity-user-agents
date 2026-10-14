package io.paritytech.polkadotapp.tools_integrity_api.domain.error

/**
 * Device-attestation failures, flavor-agnostic. `gp` (Play Integrity) and `vanilla` (Android
 * Keystore key attestation) classify their own causes into these variants.
 *
 * Variants stay payload-free because this reaches Compose state: a nested [Throwable] would
 * compare by identity and defeat recomposition skipping.
 */
sealed class IntegrityError(message: String) : Throwable(message) {
    /**
     * The device cannot attest at all — gp: Play Services/Store missing or outdated;
     * vanilla: keystore attestation unsupported.
     */
    data object AttestationUnavailable : IntegrityError("device cannot attest")

    /** The backend verified the attestation and refused it — rooted or custom-ROM device. */
    data object AttestationRejected : IntegrityError("attestation refused")

    /** Network, certificate-revocation-list outage or rate limit — retryable. */
    data object AttestationTransient : IntegrityError("attestation temporarily unavailable")

    data object Unknown : IntegrityError("attestation failed")

    // Variants are singletons, so a captured trace would point at classloading, not the failure.
    override fun fillInStackTrace(): Throwable = this
}
