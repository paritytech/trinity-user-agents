package io.paritytech.polkadotapp.common.utils

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.spec.ECGenParameterSpec

object CertificateChainProvider {
    private const val ANDROID_KEYSTORE = "AndroidKeyStore"
    private const val ALIAS = "PolkadotAppAlias"

    fun getCertificateChain(challenge: ByteArray): Result<List<String>> {
        return generateKey(challenge)
            .flatMap {
                getChain()
            }
            .mapCatching {
                it.map { cert ->
                    cert.encoded.base64NoWrap()
                }
            }
    }

    private fun generateKey(challenge: ByteArray) = runCatching {
        val spec = KeyGenParameterSpec.Builder(
            ALIAS,
            KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY
        )
            .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
            .setDigests(KeyProperties.DIGEST_SHA256)
            .setAttestationChallenge(challenge)
            .build()

        KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, ANDROID_KEYSTORE).apply {
            initialize(spec)
            generateKeyPair()
        }
    }

    private fun getChain(): Result<Array<out java.security.cert.Certificate>> = runCatching {
        KeyStore.getInstance(ANDROID_KEYSTORE)
            .apply { load(null) }
            .getCertificateChain(ALIAS)
            ?: error("No certificate chain for alias=$ALIAS")
    }
}
