package io.paritytech.polkadotapp.tools_jwt_auth_impl.data.interceptor

import io.paritytech.polkadotapp.tools_jwt_auth_api.CallWithBearerToken
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.manager.JWTTokenProvider
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import okhttp3.Interceptor
import okhttp3.Response
import retrofit2.Invocation
import java.io.IOException

/**
 * Attaches `Authorization: Bearer <token>` to outgoing requests whose Retrofit method is
 * annotated with [CallWithBearerToken]. The 401 refresh-and-retry contract lives in
 * [BearerTokenAuthenticator]; this interceptor handles only the happy path.
 */
internal class BearerTokenInterceptor(
    private val tokenProvider: JWTTokenProvider,
) : Interceptor {
    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()

        val needsBearer = request.tag(Invocation::class.java)
            ?.method()?.annotations?.any { it is CallWithBearerToken } == true

        if (!needsBearer) return chain.proceed(request)

        val token = try {
            runBlocking { tokenProvider.validToken() }
        } catch (e: Exception) {
            when (e) {
                is CancellationException, is IOException -> throw e
                else -> throw IOException("JWT acquisition failed", e)
            }
        }

        val authedRequest = request.newBuilder()
            .header(HEADER_AUTHORIZATION, "$BEARER_PREFIX$token")
            .build()

        return chain.proceed(authedRequest)
    }

    companion object {
        internal const val HEADER_AUTHORIZATION = "Authorization"
        internal const val BEARER_PREFIX = "Bearer "
    }
}
