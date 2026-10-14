package io.paritytech.polkadotapp.tools_integrity_impl.data.integrity

import android.content.Context
import io.novasama.substrate_sdk_android.encrypt.Sr25519
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.common.data.keypair.ClientKeypairStore
import io.paritytech.polkadotapp.common.utils.base64NoWrap
import io.paritytech.polkadotapp.common.utils.decodeBase64toByteArray
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapErrorNotInstance
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.common.utils.sha256
import io.paritytech.polkadotapp.common.utils.toByteArray
import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import io.paritytech.polkadotapp.tools_integrity_impl.data.api.IntegrityApi
import okhttp3.Request

abstract class IntegrityParamsInjector(
    private val applicationContext: Context,
    private val keypairStore: ClientKeypairStore,
) {
    companion object {
        private const val HEADER_CHALLENGE = "Auth-Challenge"

        private const val HEADER_CLIENT_ID = "Auth-ClientId"

        private const val HEADER_CLIENT_PROOF = "Auth-ClientProof"

        private const val HEADER_PACKAGE = "Auth-Android-Package"

        const val HEADER_ATTESTATION_TYPE = "Auth-Attestation-Type"
    }

    suspend operator fun invoke(
        requestBuilder: Request.Builder,
        integrityApi: IntegrityApi,
        challenge: String
    ): Result<Unit> {
        requestBuilder.addHeader(HEADER_CHALLENGE, challenge)
        requestBuilder.addHeader(HEADER_PACKAGE, applicationContext.packageName)

        return runCancellableCatching { keypairStore.getOrGenerate() }
            .mapCatching { keypair ->
                requestBuilder.addHeader(HEADER_CLIENT_ID, keypair.publicKey.base64NoWrap())
                keypair
            }
            .flatMap { keypair ->
                injectAttestationParams(requestBuilder, integrityApi, challenge)
                    .flatMap {
                        injectClientProof(
                            challenge = challenge,
                            keypair = keypair,
                            requestBuilder = requestBuilder,
                        )
                    }
            }
            // Attestation failures are already classified by the flavor; everything else is ours.
            .mapErrorNotInstance<Unit, IntegrityError> { IntegrityError.Unknown }
    }

    abstract suspend fun injectAttestationParams(
        requestBuilder: Request.Builder,
        integrityApi: IntegrityApi,
        challenge: String
    ): Result<Unit>

    private fun injectClientProof(
        challenge: String,
        keypair: Sr25519Keypair,
        requestBuilder: Request.Builder
    ): Result<Unit> = challenge.decodeBase64toByteArray()
        .mapCatching { challengeBytes ->
            val bodyBytes = requestBuilder.build().body?.toByteArray() ?: ByteArray(0)
            val proofPayload = (challengeBytes + keypair.publicKey + bodyBytes.sha256()).sha256()
            val signature = Sr25519.sign(keypair.publicKey, keypair.privateKey + keypair.nonce, proofPayload)

            requestBuilder.addHeader(HEADER_CLIENT_PROOF, signature.base64NoWrap())
        }
}
