package io.paritytech.polkadotapp.tools_integrity_impl.data

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.keypair.ClientKeypairStore
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.decodeBase64toByteArray
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.common.utils.sha256
import io.paritytech.polkadotapp.common.utils.sha256UrlSafe
import io.paritytech.polkadotapp.common.utils.toByteArray
import io.paritytech.polkadotapp.tools_integrity_impl.data.api.IntegrityApi
import io.paritytech.polkadotapp.tools_integrity_impl.data.integrity.IntegrityParamsInjector
import io.paritytech.polkadotapp.tools_integrity_impl.data.integrity.PlayIntegrityManager
import okhttp3.Request
import javax.inject.Inject

class RealGPIntegrityParamsInjector @Inject constructor(
    @param:ApplicationContext private val applicationContext: Context,
    private val keypairStore: ClientKeypairStore,
    private val playIntegrityManager: PlayIntegrityManager,
) : IntegrityParamsInjector(applicationContext, keypairStore) {

    companion object {
        private const val ATTESTATION_TYPE = "play-integrity"
        private const val HEADER_PAYLOAD = "Auth-Payload"
    }

    override suspend fun injectAttestationParams(
        requestBuilder: Request.Builder,
        integrityApi: IntegrityApi,
        challenge: String
    ): Result<Unit> {
        requestBuilder.addHeader(HEADER_ATTESTATION_TYPE, ATTESTATION_TYPE)

        return challenge.decodeBase64toByteArray()
            .mapCatching { challengeBytes ->
                val publicKey = keypairStore.getOrGenerate().publicKey
                val bodyEncodedBytes = (requestBuilder.build().body?.toByteArray() ?: ByteArray(0)).sha256()
                (challengeBytes + publicKey + bodyEncodedBytes).sha256UrlSafe()
            }
            .flatMap { nonce -> playIntegrityManager.generateIntegrityToken(nonce) }
            .map { integrityPayload ->
                requestBuilder.addHeader(HEADER_PAYLOAD, integrityPayload)
            }
            .coerceToUnit()
            .mapError(Throwable::toIntegrityError)
    }
}
