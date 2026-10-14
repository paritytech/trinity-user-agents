package io.paritytech.polkadotapp.tools_jwt_auth_impl.data.manager

import com.google.gson.Gson
import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.AuthError
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.DelayableAuthTokenApi
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.FakeTimeProvider
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.api.AuthTokenApi
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.createFakeTokenStore
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.makeValidJWT
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.JWTTokenResponse
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.JwtRequest
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.RefreshTokenRequest
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.parser.JWTParser
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.store.JWTTokenStore
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.yield
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify
import org.mockito.Mockito.`when`
import retrofit2.HttpException
import retrofit2.Response
import kotlin.time.ExperimentalTime

@OptIn(ExperimentalCoroutinesApi::class, ExperimentalTime::class)
class JWTTokenProviderTest {
    private lateinit var tokenStore: JWTTokenStore
    private lateinit var authTokenApi: AuthTokenApi
    private lateinit var timeProvider: FakeTimeProvider
    private lateinit var jwtParser: JWTParser
    private lateinit var manager: JWTTokenProvider

    @Before
    fun setUp() {
        tokenStore = createFakeTokenStore()
        authTokenApi = mock(AuthTokenApi::class.java)
        timeProvider = FakeTimeProvider()
        jwtParser = JWTParser(Gson())
        manager = JWTTokenProvider(tokenStore, authTokenApi, timeProvider, jwtParser, Gson())
    }

    // MARK: - Cached token

    @Test
    fun `returns cached token when available and not expired`() = runTest {
        tokenStore.saveToken(makeValidJWT(exp = 2_000_000_000L))
        timeProvider.nowSeconds = 1_000_000_000L

        val token = manager.validToken()

        assertEquals(tokenStore.fetchToken(), token)
        verify(authTokenApi, never()).fetchToken(JwtRequest)
    }

    @Test
    fun `expired cached token triggers fetch via attestation`() = runTest {
        tokenStore.saveToken(makeValidJWT(exp = 1_000L))
        timeProvider.nowSeconds = 2_000L

        val freshResponse = JWTTokenResponse("fresh-token", "fresh-refresh")
        `when`(authTokenApi.fetchToken(JwtRequest)).thenReturn(freshResponse)

        val token = manager.validToken()

        assertEquals("fresh-token", token)
        assertEquals("fresh-token", tokenStore.fetchToken())
        assertEquals("fresh-refresh", tokenStore.fetchRefreshToken())
    }

    @Test
    fun `token within buffer window triggers fetch`() = runTest {
        val exp = 1_000_100L
        tokenStore.saveToken(makeValidJWT(exp = exp))
        timeProvider.nowSeconds = exp - JWTTokenProvider.EXPIRY_BUFFER.inWholeSeconds // exactly at buffer edge

        val freshResponse = JWTTokenResponse("new-token", "new-refresh")
        `when`(authTokenApi.fetchToken(JwtRequest)).thenReturn(freshResponse)

        val token = manager.validToken()

        assertEquals("new-token", token)
    }

    // MARK: - Invalidation (compare-before-evict)

    @Test
    fun `invalidateAccessToken clears access token when used token matches store`() = runTest {
        tokenStore.saveToken("access")
        tokenStore.saveRefreshToken("refresh")

        manager.invalidateAccessToken(usedToken = "access")

        assertNull(tokenStore.fetchToken())
        // Refresh token preserved so the next validToken() can use the cheap refresh path.
        assertEquals("refresh", tokenStore.fetchRefreshToken())
    }

    @Test
    fun `invalidateAccessToken keeps token when store already holds a different one`() = runTest {
        tokenStore.saveToken("newer-access")
        tokenStore.saveRefreshToken("refresh")

        // A concurrent caller already refreshed the token to "newer-access". This call
        // tried to evict its stale "older-access" — it must NOT clobber the newer one.
        manager.invalidateAccessToken(usedToken = "older-access")

        assertEquals("newer-access", tokenStore.fetchToken())
        assertEquals("refresh", tokenStore.fetchRefreshToken())
    }

    @Test
    fun `invalidateAccessToken waits for in-flight obtainToken then evicts only if it matches`() = runTest {
        val api = DelayableAuthTokenApi()
        val m = JWTTokenProvider(tokenStore, api, timeProvider, jwtParser, Gson())
        timeProvider.nowSeconds = 1_000_000_000L

        // Kick off obtainToken — it suspends inside DelayableAuthTokenApi.fetchToken.
        val obtainJob = async { m.validToken() }
        yield() // let obtainJob acquire the Mutex and enter fetchToken

        // Invalidate is parked on the Mutex; once obtain persists "in-flight-token" the compare
        // check sees a mismatch (we passed "stale-token") and does nothing.
        val invalidateJob = async { m.invalidateAccessToken(usedToken = "stale-token") }
        yield()

        api.completeFetchToken(JWTTokenResponse("in-flight-token", "in-flight-refresh"))

        obtainJob.await()
        invalidateJob.await()

        assertEquals("in-flight-token", tokenStore.fetchToken())
        assertEquals("in-flight-refresh", tokenStore.fetchRefreshToken())
    }

    // MARK: - Coalescing

    @Test
    fun `concurrent validToken calls share a single network fetch`() = runTest {
        val api = DelayableAuthTokenApi()
        val m = JWTTokenProvider(tokenStore, api, timeProvider, jwtParser, Gson())
        timeProvider.nowSeconds = 1_000_000_000L

        // Valid JWT so the cache-recheck inside the Mutex treats it as live
        // (string tokens would fail parsing and trigger a refetch).
        val sharedToken = makeValidJWT(exp = 2_000_000_000L)

        val a = async { m.validToken() }
        val b = async { m.validToken() }
        val c = async { m.validToken() }
        yield() // let all three reach the Mutex; one enters fetchToken, two park

        api.completeFetchToken(JWTTokenResponse(sharedToken, "shared-refresh"))

        val results = awaitAll(a, b, c)

        assertEquals(listOf(sharedToken, sharedToken, sharedToken), results)
        assertEquals(1, api.fetchTokenCallCount)
    }

    // MARK: - Refresh token flow

    @Test
    fun `prefers refresh token over full attestation`() = runTest {
        tokenStore.saveRefreshToken("valid-refresh")
        timeProvider.nowSeconds = 1_000_000_000L

        val refreshResponse = JWTTokenResponse("refreshed-token", "new-refresh")
        `when`(authTokenApi.refreshToken(RefreshTokenRequest("valid-refresh")))
            .thenReturn(refreshResponse)

        val token = manager.validToken()

        assertEquals("refreshed-token", token)
        assertEquals("new-refresh", tokenStore.fetchRefreshToken())
        verify(authTokenApi, never()).fetchToken(JwtRequest)
    }

    @Test
    fun `falls back to attestation when refresh fails with 401`() = runTest {
        tokenStore.saveRefreshToken("bad-refresh")
        timeProvider.nowSeconds = 1_000_000_000L

        `when`(authTokenApi.refreshToken(RefreshTokenRequest("bad-refresh")))
            .thenThrow(HttpException(Response.error<Any>(401, "".toResponseBody())))

        val attestResponse = JWTTokenResponse("attest-token", "attest-refresh")
        `when`(authTokenApi.fetchToken(JwtRequest)).thenReturn(attestResponse)

        val token = manager.validToken()

        assertEquals("attest-token", token)
        // Refresh token was dropped before attestation; attestation persisted a new one.
        assertEquals("attest-refresh", tokenStore.fetchRefreshToken())
    }

    @Test
    fun `saves both tokens after successful attestation`() = runTest {
        timeProvider.nowSeconds = 1_000_000_000L
        val response = JWTTokenResponse("new-access", "new-refresh")
        `when`(authTokenApi.fetchToken(JwtRequest)).thenReturn(response)

        manager.validToken()

        assertEquals("new-access", tokenStore.fetchToken())
        assertEquals("new-refresh", tokenStore.fetchRefreshToken())
    }

    // MARK: - Refresh failure classification (refreshTokenIsUnusable)
    //
    // Each case stubs `refreshToken` to throw a specific failure, then leaves
    // `fetchToken` unstubbed (Mockito returns null → attestation throws NPE inside
    // persistTokens), so `validToken()` propagates an exception. We assert the
    // post-state of the refresh token — dropped (null) or kept (original) — to verify
    // the classifier without depending on the attestation result.

    @Test
    fun `refresh failure 400 keeps refresh token`() = runTest {
        // Identity Backend spec says refresh emits only 200 / 401 / 500. A 400 from a proxy / CDN
        // / gateway is anomalous; the spec-aligned classifier treats anything not-401 as transient
        // — keep the refresh and let the next call retry.
        tokenStore.saveRefreshToken("rt-400")
        timeProvider.nowSeconds = 1_000_000_000L

        `when`(authTokenApi.refreshToken(RefreshTokenRequest("rt-400")))
            .thenThrow(HttpException(Response.error<Any>(400, "".toResponseBody())))

        runCatching { manager.validToken() }

        assertEquals("rt-400", tokenStore.fetchRefreshToken())
    }

    @Test
    fun `refresh failure 408 keeps refresh token`() = runTest {
        tokenStore.saveRefreshToken("rt-408")
        timeProvider.nowSeconds = 1_000_000_000L

        `when`(authTokenApi.refreshToken(RefreshTokenRequest("rt-408")))
            .thenThrow(HttpException(Response.error<Any>(408, "".toResponseBody())))

        runCatching { manager.validToken() }

        assertEquals("rt-408", tokenStore.fetchRefreshToken())
    }

    @Test
    fun `refresh failure 429 keeps refresh token`() = runTest {
        tokenStore.saveRefreshToken("rt-429")
        timeProvider.nowSeconds = 1_000_000_000L

        `when`(authTokenApi.refreshToken(RefreshTokenRequest("rt-429")))
            .thenThrow(HttpException(Response.error<Any>(429, "".toResponseBody())))

        runCatching { manager.validToken() }

        assertEquals("rt-429", tokenStore.fetchRefreshToken())
    }

    @Test
    fun `refresh failure 500 keeps refresh token`() = runTest {
        tokenStore.saveRefreshToken("rt-500")
        timeProvider.nowSeconds = 1_000_000_000L

        `when`(authTokenApi.refreshToken(RefreshTokenRequest("rt-500")))
            .thenThrow(HttpException(Response.error<Any>(500, "".toResponseBody())))

        runCatching { manager.validToken() }

        assertEquals("rt-500", tokenStore.fetchRefreshToken())
    }

    @Test
    fun `refresh failure with non-HttpException keeps refresh token`() = runTest {
        // IOException would be the realistic case (network down), but Mockito's `thenThrow`
        // rejects checked exceptions not declared on the method signature; the classifier's
        // `this !is HttpException -> false` short-circuit applies equally to RuntimeException.
        tokenStore.saveRefreshToken("rt-other")
        timeProvider.nowSeconds = 1_000_000_000L

        `when`(authTokenApi.refreshToken(RefreshTokenRequest("rt-other")))
            .thenThrow(RuntimeException("network down"))

        runCatching { manager.validToken() }

        assertEquals("rt-other", tokenStore.fetchRefreshToken())
    }

    // MARK: - Attestation error classification

    @Test
    fun `attestation 403 with INTEGRITY_FAILED code throws AttestationRejected`() = runTest {
        timeProvider.nowSeconds = 1_000_000_000L
        `when`(authTokenApi.fetchToken(JwtRequest))
            .thenThrow(httpException(403, """{"error":"INTEGRITY_FAILED","message":"attestation rejected"}"""))

        val error = runCatching { manager.validToken() }.exceptionOrNull()

        assertEquals(AuthError.Integrity(IntegrityError.AttestationRejected), error)
    }

    @Test
    fun `attestation 403 without INTEGRITY_FAILED code propagates HttpException`() = runTest {
        timeProvider.nowSeconds = 1_000_000_000L
        `when`(authTokenApi.fetchToken(JwtRequest))
            .thenThrow(httpException(403, """{"error":"FORBIDDEN","message":"forbidden"}"""))

        val error = runCatching { manager.validToken() }.exceptionOrNull()

        assertTrue("expected HttpException but was $error", error is HttpException)
    }

    @Test
    fun `attestation 503 CRL unavailable retries once and succeeds`() = runTest {
        timeProvider.nowSeconds = 1_000_000_000L
        `when`(authTokenApi.fetchToken(JwtRequest))
            .thenThrow(httpException(503, """{"error":"ATTESTATION_CRL_UNAVAILABLE","message":"retry"}"""))
            .thenReturn(JWTTokenResponse("retried-token", "retried-refresh"))

        val token = manager.validToken()

        assertEquals("retried-token", token)
        assertEquals("retried-refresh", tokenStore.fetchRefreshToken())
    }

    @Test
    fun `attestation 503 CRL unavailable on retry throws AttestationTransient`() = runTest {
        timeProvider.nowSeconds = 1_000_000_000L
        `when`(authTokenApi.fetchToken(JwtRequest))
            .thenThrow(
                httpException(503, """{"error":"ATTESTATION_CRL_UNAVAILABLE","message":"retry"}"""),
                httpException(503, """{"error":"ATTESTATION_CRL_UNAVAILABLE","message":"retry"}""")
            )

        val error = runCatching { manager.validToken() }.exceptionOrNull()

        // The retry is spent, but the failure is still the retryable kind - the user should be
        // told to try again, not that their device is rooted.
        assertEquals(AuthError.Integrity(IntegrityError.AttestationTransient), error)
    }

    private fun httpException(code: Int, body: String): HttpException {
        return HttpException(Response.error<Any>(code, body.toResponseBody()))
    }

    // MARK: - Expiry

    @Test
    fun `malformed JWT is treated as expired`() = runTest {
        tokenStore.saveToken("not-a-jwt")
        timeProvider.nowSeconds = 1_000_000_000L

        val response = JWTTokenResponse("valid-token", "valid-refresh")
        `when`(authTokenApi.fetchToken(JwtRequest)).thenReturn(response)

        val token = manager.validToken()

        assertEquals("valid-token", token)
        assertNotNull(tokenStore.fetchToken())
    }
}
