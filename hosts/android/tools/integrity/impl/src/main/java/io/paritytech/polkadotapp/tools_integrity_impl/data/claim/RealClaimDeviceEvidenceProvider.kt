package io.paritytech.polkadotapp.tools_integrity_impl.data.claim

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import io.paritytech.polkadotapp.common.data.keypair.ClientKeypairStore
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.base64NoWrap
import io.paritytech.polkadotapp.common.utils.decodeBase64toByteArray
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flatRecover
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.tools_integrity_api.claim.ClaimDeviceEvidence
import io.paritytech.polkadotapp.tools_integrity_api.claim.ClaimDeviceEvidenceProvider
import io.paritytech.polkadotapp.tools_integrity_impl.data.api.IntegrityApi
import kotlinx.coroutines.withContext
import timber.log.Timber
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.spec.ECGenParameterSpec
import java.util.UUID
import javax.inject.Inject

class RealClaimDeviceEvidenceProvider @Inject constructor(
    private val integrityApi: IntegrityApi,
    private val clientKeypairStore: ClientKeypairStore,
    private val coroutineDispatchers: CoroutineDispatchers
) : ClaimDeviceEvidenceProvider {
    override suspend fun collectEvidence(): Result<ClaimDeviceEvidence?> = withContext(coroutineDispatchers.io) {
        fetchClaimChallenge()
            .mapCatching { challenge -> buildEvidence(challenge) }
            .flatRecover { error ->
                if (error is WidevineUnavailableException) {
                    Timber.w(error, "Widevine evidence unavailable")
                    Result.success(null)
                } else {
                    Result.failure(error)
                }
            }
            .logFailure("Failed to collect claim device evidence")
    }

    // Claim evidence needs its own challenge.
    private suspend fun fetchClaimChallenge(): Result<ByteArray> {
        return runCancellableCatching { integrityApi.fetchChallenge().challenge }
            .flatMap { it.decodeBase64toByteArray() }
            .mapCatching { challenge ->
                require(challenge.size == CHALLENGE_BYTES) {
                    "claim challenge must be $CHALLENGE_BYTES bytes, got ${challenge.size}"
                }
                challenge
            }
    }

    private fun buildEvidence(challenge: ByteArray): ClaimDeviceEvidence? {
        val candidate = clientKeypairStore.getOrGenerate().publicKey
        require(candidate.size == CANDIDATE_BYTES) {
            "client public key must be $CANDIDATE_BYTES bytes, got ${candidate.size}"
        }
        val rawWidevineId = WidevineEvidenceReader.readL1DeviceId() ?: return null
        val deviceId = ClaimEvidenceDigests.deviceId(rawWidevineId)
        val attestationChallenge = ClaimEvidenceDigests.attestationChallenge(
            challenge = challenge,
            candidate = candidate,
            deviceId = deviceId
        )
        val alias = "$KEY_ALIAS_PREFIX${UUID.randomUUID()}"
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        try {
            generateAttestedKey(alias, attestationChallenge, keyStore)
            return ClaimDeviceEvidence(
                attestationChain = attestationChain(keyStore, alias),
                deviceChallenge = challenge.base64NoWrap(),
                deviceId = deviceId.base64NoWrap()
            )
        } finally {
            runCatching { keyStore.deleteEntry(alias) }
        }
    }

    private fun attestationChain(keyStore: KeyStore, alias: String): List<String> {
        val chain = requireNotNull(keyStore.getCertificateChain(alias)) {
            "Android Keystore returned no attestation chain"
        }.map { certificate -> certificate.encoded.base64NoWrap() }
        require(chain.size in MIN_CHAIN_ENTRIES..MAX_CHAIN_ENTRIES) {
            "attestation chain length must be $MIN_CHAIN_ENTRIES..$MAX_CHAIN_ENTRIES, got ${chain.size}"
        }
        require(chain.all { it.length <= MAX_CHAIN_ENTRY_CHARS }) {
            "attestation chain entry exceeds $MAX_CHAIN_ENTRY_CHARS chars"
        }
        return chain
    }

    private fun generateAttestedKey(alias: String, challenge: ByteArray, keyStore: KeyStore) {
        try {
            generateKey(alias, challenge, strongBox = true)
            return
        } catch (error: Exception) {
            runCatching { keyStore.deleteEntry(alias) }
        }
        generateKey(alias, challenge, strongBox = false)
    }

    private fun generateKey(alias: String, challenge: ByteArray, strongBox: Boolean) {
        val spec = KeyGenParameterSpec.Builder(
            alias,
            KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY
        )
            .setAlgorithmParameterSpec(ECGenParameterSpec(EC_CURVE))
            .setDigests(KeyProperties.DIGEST_SHA256)
            .setAttestationChallenge(challenge)
            .apply { if (strongBox) setIsStrongBoxBacked(true) }
            .build()
        KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, ANDROID_KEYSTORE).run {
            initialize(spec)
            generateKeyPair()
        }
    }

    private companion object {
        const val ANDROID_KEYSTORE = "AndroidKeyStore"
        const val EC_CURVE = "secp256r1"
        const val KEY_ALIAS_PREFIX = "polkadotapp-claim-attestation-"
        const val CHALLENGE_BYTES = 32
        const val CANDIDATE_BYTES = 32
        const val MIN_CHAIN_ENTRIES = 2
        const val MAX_CHAIN_ENTRIES = 10
        const val MAX_CHAIN_ENTRY_CHARS = 8192
    }
}
