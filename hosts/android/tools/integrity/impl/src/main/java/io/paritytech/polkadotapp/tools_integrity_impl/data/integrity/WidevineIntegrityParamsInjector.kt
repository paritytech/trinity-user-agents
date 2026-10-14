package io.paritytech.polkadotapp.tools_integrity_impl.data.integrity

import com.google.gson.Gson
import com.google.gson.JsonObject
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.common.utils.toByteArray
import io.paritytech.polkadotapp.tools_integrity_api.claim.ClaimDeviceEvidence
import io.paritytech.polkadotapp.tools_integrity_api.claim.ClaimDeviceEvidenceProvider
import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import kotlinx.coroutines.CancellationException
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import retrofit2.HttpException
import java.io.IOException
import java.security.InvalidAlgorithmParameterException
import java.security.KeyStoreException
import java.security.ProviderException
import javax.inject.Inject

class WidevineIntegrityParamsInjector @Inject constructor(
    private val evidenceProvider: ClaimDeviceEvidenceProvider,
    private val gson: Gson,
) {
    suspend operator fun invoke(request: Request): Result<Request> {
        return evidenceProvider.collectEvidence()
            .mapError(Throwable::toClaimIntegrityError)
            .flatMap { evidence ->
                if (evidence == null) {
                    Result.success(request)
                } else {
                    injectEvidence(request, evidence)
                        .mapError { IntegrityError.Unknown }
                }
            }
    }

    private fun injectEvidence(request: Request, evidence: ClaimDeviceEvidence): Result<Request> =
        runCancellableCatching {
            val originalBody = requireNotNull(request.body)
            val bodyJson = gson.fromJson(
                originalBody.toByteArray().toString(Charsets.UTF_8),
                JsonObject::class.java,
            )
            bodyJson.add(FIELD_ATTESTATION_CHAIN, gson.toJsonTree(evidence.attestationChain))
            bodyJson.addProperty(FIELD_DEVICE_CHALLENGE, evidence.deviceChallenge)
            bodyJson.addProperty(FIELD_DEVICE_ID, evidence.deviceId)
            val replacementBody = gson.toJson(bodyJson).toRequestBody(originalBody.contentType())

            request.newBuilder()
                .method(request.method, replacementBody)
                .build()
        }

    private companion object {
        const val FIELD_ATTESTATION_CHAIN = "attestationChain"
        const val FIELD_DEVICE_CHALLENGE = "deviceChallenge"
        const val FIELD_DEVICE_ID = "deviceId"
    }
}

private fun Throwable.toClaimIntegrityError(): IntegrityError = when (this) {
    is CancellationException -> throw this
    is IntegrityError -> this
    is IOException,
    is HttpException -> IntegrityError.AttestationTransient

    is ProviderException,
    is InvalidAlgorithmParameterException,
    is KeyStoreException,
    is UnsupportedOperationException -> IntegrityError.AttestationUnavailable

    else -> IntegrityError.Unknown
}
