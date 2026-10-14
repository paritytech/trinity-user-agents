package io.paritytech.polkadotapp.tools_integrity_impl.data.interceptors

import androidx.annotation.Keep
import com.google.gson.Gson
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.kilobytes
import io.paritytech.polkadotapp.tools_integrity_api.interceptors.CallWithWidevineIntegrity
import io.paritytech.polkadotapp.tools_integrity_api.interceptors.WidevineIntegrityInterceptor
import io.paritytech.polkadotapp.tools_integrity_impl.data.integrity.WidevineIntegrityParamsInjector
import kotlinx.coroutines.runBlocking
import okhttp3.Interceptor
import okhttp3.Request
import okhttp3.Response
import retrofit2.Invocation
import java.io.IOException
import javax.inject.Inject

class RealWidevineIntegrityInterceptor @Inject constructor(
    private val paramsInjector: WidevineIntegrityParamsInjector,
    private val gson: Gson,
) : WidevineIntegrityInterceptor {
    override fun intercept(chain: Interceptor.Chain): Response {
        val originalRequest = chain.request()
        if (!originalRequest.needsWidevineIntegrity()) return chain.proceed(originalRequest)

        val firstResponse = chain.proceed(injectParams(originalRequest))
        if (!firstResponse.isDeviceEvidenceInvalid()) return firstResponse
        firstResponse.close()

        val secondResponse = chain.proceed(injectParams(originalRequest))
        if (!secondResponse.isDeviceEvidenceInvalid()) return secondResponse
        secondResponse.close()

        return chain.proceed(originalRequest)
    }

    private fun injectParams(request: Request): Request = runBlocking {
        paramsInjector(request).getOrElse { error ->
            throw IOException("Failed to attach Widevine claim evidence", error)
        }
    }

    private fun Request.needsWidevineIntegrity(): Boolean {
        return tag(Invocation::class.java)
            ?.method()?.annotations?.any { it is CallWithWidevineIntegrity } == true
    }

    private fun Response.isDeviceEvidenceInvalid(): Boolean {
        if (code != HTTP_FORBIDDEN) return false
        val errorBody = runCatching { peekBody(MAX_ERROR_BODY_SIZE.inWholeBytes).string() }.getOrNull() ?: return false
        val parsed = runCatching {
            gson.fromJson(errorBody, ClaimErrorResponse::class.java)
        }.getOrNull()
        return parsed?.error == DEVICE_EVIDENCE_INVALID
    }

    private companion object {
        const val HTTP_FORBIDDEN = 403
        const val DEVICE_EVIDENCE_INVALID = "DEVICE_EVIDENCE_INVALID"
        val MAX_ERROR_BODY_SIZE = 64.kilobytes
    }
}

@Keep
private class ClaimErrorResponse(
    val error: String?,
)
