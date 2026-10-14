package io.paritytech.polkadotapp.tools_integrity_impl.data

import android.content.Context
import com.google.gson.Gson
import com.google.gson.reflect.TypeToken
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.keypair.ClientKeypairStore
import io.paritytech.polkadotapp.common.utils.CertificateChainProvider
import io.paritytech.polkadotapp.common.utils.decodeBase64toByteArray
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.common.utils.toByteArray
import io.paritytech.polkadotapp.tools_integrity_impl.data.api.IntegrityApi
import io.paritytech.polkadotapp.tools_integrity_impl.data.integrity.IntegrityParamsInjector
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import javax.inject.Inject

class RealVanillaIntegrityParamsInjector @Inject constructor(
    @param:ApplicationContext private val applicationContext: Context,
    keypairStore: ClientKeypairStore,
    private val gson: Gson
) : IntegrityParamsInjector(applicationContext, keypairStore) {

    companion object {
        private const val ATTESTATION_TYPE = "key-attestation"
        private const val FIELD_ATTESTATION_CHAIN = "attestationChain"
        private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()
        private val MAP_TYPE = object : TypeToken<MutableMap<String, Any?>>() {}.type
    }

    override suspend fun injectAttestationParams(
        requestBuilder: Request.Builder,
        integrityApi: IntegrityApi,
        challenge: String
    ): Result<Unit> {
        requestBuilder.addHeader(HEADER_ATTESTATION_TYPE, ATTESTATION_TYPE)

        return challenge.decodeBase64toByteArray()
            .flatMap { challengeBytes ->
                CertificateChainProvider.getCertificateChain(challengeBytes)
            }
            .flatMap { certChain ->
                injectAttestationField(requestBuilder, certChain)
            }
            .mapError(Throwable::toIntegrityError)
    }

    private fun injectAttestationField(requestBuilder: Request.Builder, certChain: List<String>): Result<Unit> = runCatching {
        val currentRequest = requestBuilder.build()

        val bodyString = currentRequest.body?.toByteArray()?.toString(Charsets.UTF_8).orEmpty()
        val bodyMap: MutableMap<String, Any?> = if (bodyString.isNotEmpty()) {
            gson.fromJson(bodyString, MAP_TYPE)
        } else {
            mutableMapOf()
        }
        bodyMap[FIELD_ATTESTATION_CHAIN] = certChain
        val jsonBody = gson.toJson(bodyMap).toRequestBody(JSON_MEDIA_TYPE)

        requestBuilder.method(currentRequest.method, jsonBody)
    }
}
