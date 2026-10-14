package io.paritytech.polkadotapp.tools_jwt_auth_impl.data

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.api.AuthTokenApi
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.JWTTokenResponse
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.JwtRequest
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.RefreshTokenRequest
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.store.JWTTokenStore
import kotlinx.coroutines.CompletableDeferred
import java.util.Base64
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

internal fun makeValidJWT(exp: Long): String {
    val header = Base64.getUrlEncoder().withoutPadding()
        .encodeToString("""{"alg":"HS256"}""".toByteArray())
    val body = Base64.getUrlEncoder().withoutPadding()
        .encodeToString("""{"exp":$exp}""".toByteArray())
    return "$header.$body.sig"
}

@OptIn(ExperimentalTime::class)
internal class FakeTimeProvider(initialNowSeconds: Long = 0L) : TimeProvider {
    var nowSeconds: Long = initialNowSeconds
    override fun now(): Instant = Instant.fromEpochSeconds(nowSeconds)
}

internal fun createFakeTokenStore(): JWTTokenStore {
    val prefs = object : io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences {
        private val storage = mutableMapOf<String, String>()
        override fun putEncryptedString(field: String, value: String) { storage[field] = value }
        override fun getDecryptedString(field: String): String? = storage[field]
        override fun hasKey(field: String): Boolean = storage.containsKey(field)
        override fun removeKey(field: String) { storage.remove(field) }
        override fun decryptedStringFlow(field: String) = kotlinx.coroutines.flow.flowOf(storage[field])
    }
    return JWTTokenStore(prefs)
}

internal class DelayableAuthTokenApi : AuthTokenApi {
    private val fetchTokenResponse = CompletableDeferred<JWTTokenResponse>()
    var fetchTokenCallCount: Int = 0
        private set

    fun completeFetchToken(response: JWTTokenResponse) {
        fetchTokenResponse.complete(response)
    }

    override suspend fun fetchToken(body: JwtRequest): JWTTokenResponse {
        require(body === JwtRequest)
        fetchTokenCallCount++
        return fetchTokenResponse.await()
    }

    override suspend fun refreshToken(body: RefreshTokenRequest): JWTTokenResponse =
        error("refreshToken not expected in this test")
}
