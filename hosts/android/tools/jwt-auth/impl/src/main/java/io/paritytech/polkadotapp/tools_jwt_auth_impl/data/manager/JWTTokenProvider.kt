package io.paritytech.polkadotapp.tools_jwt_auth_impl.data.manager

import com.google.gson.Gson
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.AuthError
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.api.AuthTokenApi
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.AuthErrorResponse
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.JWTTokenResponse
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.JwtRequest
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.RefreshTokenRequest
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.parser.JWTParser
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.store.JWTTokenStore
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import retrofit2.HttpException
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.seconds
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class)
@Singleton
internal class JWTTokenProvider @Inject constructor(
    private val tokenStore: JWTTokenStore,
    private val authTokenApi: AuthTokenApi,
    private val timeProvider: TimeProvider,
    private val jwtParser: JWTParser,
    private val gson: Gson,
) {
    private val mutex = Mutex()

    internal suspend fun validToken(): String {
        tokenStore.fetchToken()?.takeIf { !isExpired(it) }?.let { return it }

        return mutex.withLock {
            tokenStore.fetchToken()?.takeIf { !isExpired(it) }?.let { return@withLock it }
            obtainToken()
        }
    }

    internal suspend fun invalidateAccessToken(usedToken: String) {
        mutex.withLock {
            if (tokenStore.fetchToken() == usedToken) {
                tokenStore.deleteToken()
            }
        }
    }

    private suspend fun obtainToken(): String {
        val refreshToken = tokenStore.fetchRefreshToken()
        if (refreshToken != null) {
            runCancellableCatching { refreshAccessToken(refreshToken) }
                .onSuccess { return it }
                .onFailure { e ->
                    Timber.w(e, "Refresh token failed, falling back to attestation")
                    if (e.refreshTokenIsUnusable()) {
                        tokenStore.deleteAll()
                    }
                }
        }

        return fetchTokenViaAttestation()
    }

    private suspend fun fetchTokenViaAttestation(): String {
        val response = fetchTokenClassifyingErrors(shouldRetryOnCrlFailure = true)
        persistTokens(response.token, response.refreshToken)
        return response.token
    }

    private suspend fun fetchTokenClassifyingErrors(shouldRetryOnCrlFailure: Boolean): JWTTokenResponse {
        return try {
            authTokenApi.fetchToken(JwtRequest)
        } catch (e: HttpException) {
            if (shouldRetryOnCrlFailure && e.hasAuthError(HTTP_SERVICE_UNAVAILABLE, ERROR_CRL_UNAVAILABLE)) {
                Timber.w(e, "Attestation CRL unavailable, retrying with a fresh challenge")
                return fetchTokenClassifyingErrors(shouldRetryOnCrlFailure = false)
            }

            throw e.toAttestationError() ?: e
        }
    }

    private fun HttpException.toAttestationError(): AuthError? = when {
        hasAuthError(HTTP_FORBIDDEN, ERROR_INTEGRITY_FAILED) ->
            AuthError.Integrity(IntegrityError.AttestationRejected)

        // Only reached once the single CRL retry is spent, but it stays the retryable kind.
        hasAuthError(HTTP_SERVICE_UNAVAILABLE, ERROR_CRL_UNAVAILABLE) ->
            AuthError.Integrity(IntegrityError.AttestationTransient)

        else -> null
    }

    private fun HttpException.hasAuthError(httpCode: Int, errorCode: String): Boolean {
        if (code() != httpCode) return false

        val body = response()?.errorBody()?.string() ?: return false
        val parsed = runCatching { gson.fromJson(body, AuthErrorResponse::class.java) }.getOrNull()
        return parsed?.error == errorCode
    }

    private suspend fun refreshAccessToken(refreshToken: String): String {
        val response = authTokenApi.refreshToken(RefreshTokenRequest(refreshToken))
        persistTokens(response.token, response.refreshToken)
        return response.token
    }

    private fun persistTokens(accessToken: String, refreshToken: String) {
        tokenStore.saveToken(accessToken)
        tokenStore.saveRefreshToken(refreshToken)
    }

    private fun isExpired(token: String): Boolean {
        val exp = jwtParser.parse(token).getOrNull()?.exp ?: return true
        val expiry = Instant.fromEpochSeconds(exp)
        return timeProvider.now() + EXPIRY_BUFFER >= expiry
    }

    private fun Throwable.refreshTokenIsUnusable(): Boolean {
        return this is HttpException && code() in listOf(HTTP_UNAUTHORIZED, HTTP_FORBIDDEN)
    }

    companion object {
        internal val EXPIRY_BUFFER = 30.seconds
        private const val HTTP_UNAUTHORIZED = 401
        private const val HTTP_FORBIDDEN = 403
        private const val HTTP_SERVICE_UNAVAILABLE = 503
        private const val ERROR_INTEGRITY_FAILED = "INTEGRITY_FAILED"

        // Play Integrity attestation is verified against Google's certificate revocation list (CRL);
        // the backend returns this when it cannot fetch the CRL, which a retry usually resolves
        private const val ERROR_CRL_UNAVAILABLE = "ATTESTATION_CRL_UNAVAILABLE"
    }
}
