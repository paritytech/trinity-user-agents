package io.paritytech.polkadotapp.tools_jwt_auth_impl.data.interceptor

import io.paritytech.polkadotapp.tools_jwt_auth_api.CallWithBearerToken
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.interceptor.BearerTokenInterceptor.Companion.BEARER_PREFIX
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.interceptor.BearerTokenInterceptor.Companion.HEADER_AUTHORIZATION
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.manager.JWTTokenProvider
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.runBlocking
import okhttp3.Authenticator
import okhttp3.Request
import okhttp3.Response
import okhttp3.Route
import retrofit2.Invocation
import timber.log.Timber
import java.io.IOException

/**
 * OkHttp [Authenticator] invoked when a request annotated with [CallWithBearerToken] returns 401.
 */
internal class BearerTokenAuthenticator(
    private val tokenProvider: JWTTokenProvider,
) : Authenticator {
    override fun authenticate(route: Route?, response: Response): Request? {
        val request = response.request
        val needsBearer = request.tag(Invocation::class.java)
            ?.method()
            ?.annotations
            ?.any { it is CallWithBearerToken } == true

        if (!needsBearer) return null

        if (response.responseCount() > MAX_AUTHENTICATE_ATTEMPTS) {
            Timber.w("BearerTokenAuthenticator: giving up after %d attempts on %s", MAX_AUTHENTICATE_ATTEMPTS, request.url.encodedPath)
            return null
        }

        val usedToken = request.header(HEADER_AUTHORIZATION)?.removePrefix(BEARER_PREFIX).orEmpty()
        if (usedToken.isEmpty()) return null

        val freshToken = runCatching {
            runBlocking {
                tokenProvider.invalidateAccessToken(usedToken)
                tokenProvider.validToken()
            }
        }.getOrElse { e ->
            when (e) {
                is CancellationException,
                is IOException -> throw e
                else -> {
                    Timber.w(e, "BearerTokenAuthenticator: refresh failed on %s", request.url.encodedPath)
                    // Returning null here dropped the cause, leaving the caller with a bare 401
                    // and no way to tell an expired JWT from a rejected attestation.
                    throw IOException("JWT refresh failed", e)
                }
            }
        }

        if (freshToken == usedToken) {
            return null
        }

        Timber.d("BearerTokenAuthenticator: refreshed token after 401 on %s", request.url.encodedPath)
        return request.newBuilder()
            .header(HEADER_AUTHORIZATION, "$BEARER_PREFIX$freshToken")
            .build()
    }

    private companion object {
        const val MAX_AUTHENTICATE_ATTEMPTS = 1
    }
}

private fun Response.responseCount(): Int =
    generateSequence(this) { it.priorResponse }.count()
