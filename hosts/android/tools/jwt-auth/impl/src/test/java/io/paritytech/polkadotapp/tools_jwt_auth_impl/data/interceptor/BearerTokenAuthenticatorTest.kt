package io.paritytech.polkadotapp.tools_jwt_auth_impl.data.interceptor

import com.google.gson.Gson
import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import io.paritytech.polkadotapp.tools_jwt_auth_api.CallWithBearerToken
import io.paritytech.polkadotapp.tools_jwt_auth_api.domain.error.AuthError
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.DelayableAuthTokenApi
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.FakeTimeProvider
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.api.AuthTokenApi
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.createFakeTokenStore
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.makeValidJWT
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.manager.JWTTokenProvider
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.JWTTokenResponse
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.JwtRequest
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.parser.JWTParser
import kotlinx.coroutines.runBlocking
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.`when`
import retrofit2.Invocation
import java.io.IOException
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

class BearerTokenAuthenticatorTest {
    @Test
    fun `produces retried request with fresh bearer token on 401`() {
        val tokenStore = createFakeTokenStore()
        val staleJwt = makeValidJWT(exp = 2_000_000_000L)
        tokenStore.saveToken(staleJwt)
        val authTokenApi = mock(AuthTokenApi::class.java)
        val freshJwt = makeValidJWT(exp = 3_000_000_000L)
        runBlocking {
            `when`(authTokenApi.fetchToken(JwtRequest))
                .thenReturn(JWTTokenResponse(freshJwt, "fresh-refresh"))
        }
        val authenticator = BearerTokenAuthenticator(
            tokenProvider = JWTTokenProvider(
                tokenStore = tokenStore,
                authTokenApi = authTokenApi,
                timeProvider = FakeTimeProvider(1_000_000_000L),
                jwtParser = JWTParser(Gson()),
                gson = Gson(),
            ),
        )

        val originalRequest = buildAnnotatedRequest()
            .newBuilder()
            .header("Authorization", "Bearer $staleJwt")
            .build()
        val response = unauthorizedResponse(originalRequest)

        val retried = authenticator.authenticate(route = null, response = response)

        assertNotNull(retried)
        assertEquals("Bearer $freshJwt", retried?.header("Authorization"))
        assertEquals(freshJwt, tokenStore.fetchToken())
    }

    @Test
    fun `gives up when annotation absent`() {
        val tokenStore = createFakeTokenStore()
        tokenStore.saveToken(makeValidJWT(exp = 2_000_000_000L))
        val authenticator = BearerTokenAuthenticator(
            tokenProvider = JWTTokenProvider(
                tokenStore = tokenStore,
                authTokenApi = mock(AuthTokenApi::class.java),
                timeProvider = FakeTimeProvider(1_000_000_000L),
                jwtParser = JWTParser(Gson()),
                gson = Gson(),
            ),
        )

        val unannotated = Request.Builder()
            .url("https://example.com/api/test")
            .header("Authorization", "Bearer x")
            .build()

        assertNull(authenticator.authenticate(route = null, response = unauthorizedResponse(unannotated)))
    }

    @Test
    fun `gives up after one prior 401 response in chain`() {
        val tokenStore = createFakeTokenStore()
        val staleJwt = makeValidJWT(exp = 2_000_000_000L)
        tokenStore.saveToken(staleJwt)
        val authTokenApi = mock(AuthTokenApi::class.java)
        runBlocking {
            `when`(authTokenApi.fetchToken(JwtRequest))
                .thenReturn(JWTTokenResponse(makeValidJWT(exp = 3_000_000_000L), "fresh-refresh"))
        }
        val authenticator = BearerTokenAuthenticator(
            tokenProvider = JWTTokenProvider(
                tokenStore = tokenStore,
                authTokenApi = authTokenApi,
                timeProvider = FakeTimeProvider(1_000_000_000L),
                jwtParser = JWTParser(Gson()),
                gson = Gson(),
            ),
        )

        val request = buildAnnotatedRequest()
            .newBuilder()
            .header("Authorization", "Bearer $staleJwt")
            .build()
        val first = unauthorizedResponse(request)
        val second = unauthorizedResponse(request, prior = first)

        // Second 401 means our prior retry already failed. Authenticator must give up.
        assertNull(authenticator.authenticate(route = null, response = second))
    }

    @Test
    fun `refresh failure propagates the typed cause`() {
        val tokenStore = createFakeTokenStore()
        val staleJwt = makeValidJWT(exp = 2_000_000_000L)
        tokenStore.saveToken(staleJwt)
        val authTokenApi = mock(AuthTokenApi::class.java)
        runBlocking {
            // thenThrow rejects a bare Throwable subclass as a checked exception, so the
            // sealed error is raised from an answer instead.
            `when`(authTokenApi.fetchToken(JwtRequest))
                .thenAnswer { throw AuthError.Integrity(IntegrityError.AttestationRejected) }
        }
        val authenticator = BearerTokenAuthenticator(
            tokenProvider = JWTTokenProvider(
                tokenStore = tokenStore,
                authTokenApi = authTokenApi,
                timeProvider = FakeTimeProvider(1_000_000_000L),
                jwtParser = JWTParser(Gson()),
                gson = Gson(),
            ),
        )

        val request = buildAnnotatedRequest()
            .newBuilder()
            .header("Authorization", "Bearer $staleJwt")
            .build()

        // Returning null used to drop the cause, leaving the caller with a bare 401. An
        // Authenticator may throw IOException, so the typed reason rides inside one.
        val thrown = runCatching {
            authenticator.authenticate(route = null, response = unauthorizedResponse(request))
        }.exceptionOrNull()

        assertTrue("expected IOException but was $thrown", thrown is IOException)
        assertEquals(AuthError.Integrity(IntegrityError.AttestationRejected), thrown?.cause)
    }

    @Test
    fun `gives up when request lacks Authorization header`() {
        val tokenStore = createFakeTokenStore()
        tokenStore.saveToken(makeValidJWT(exp = 2_000_000_000L))
        val authenticator = BearerTokenAuthenticator(
            tokenProvider = JWTTokenProvider(
                tokenStore = tokenStore,
                authTokenApi = mock(AuthTokenApi::class.java),
                timeProvider = FakeTimeProvider(1_000_000_000L),
                jwtParser = JWTParser(Gson()),
                gson = Gson(),
            ),
        )

        // Annotated but no Authorization header: defensive path — return null.
        val request = buildAnnotatedRequest()

        assertNull(authenticator.authenticate(route = null, response = unauthorizedResponse(request)))
    }

    @Test
    fun `concurrent 401s collapse to single network refresh`() {
        val tokenStore = createFakeTokenStore()
        val staleJwt = makeValidJWT(exp = 2_000_000_000L)
        tokenStore.saveToken(staleJwt)

        val api = DelayableAuthTokenApi()
        val freshJwt = makeValidJWT(exp = 3_000_000_000L)
        val authenticator = BearerTokenAuthenticator(
            tokenProvider = JWTTokenProvider(
                tokenStore = tokenStore,
                authTokenApi = api,
                timeProvider = FakeTimeProvider(1_000_000_000L),
                jwtParser = JWTParser(Gson()),
                gson = Gson(),
            ),
        )

        val request = buildAnnotatedRequest()
            .newBuilder()
            .header("Authorization", "Bearer $staleJwt")
            .build()
        val response = unauthorizedResponse(request)

        val executor = Executors.newFixedThreadPool(2)
        val futureA = executor.submit<Request?> { authenticator.authenticate(route = null, response = response) }
        val futureB = executor.submit<Request?> { authenticator.authenticate(route = null, response = response) }

        // Give both threads time to enter runBlocking and contend on the Mutex.
        Thread.sleep(200)
        api.completeFetchToken(JWTTokenResponse(freshJwt, "fresh-refresh"))

        val retriedA = futureA.get(2, TimeUnit.SECONDS)
        val retriedB = futureB.get(2, TimeUnit.SECONDS)
        executor.shutdown()

        assertNotNull(retriedA)
        assertNotNull(retriedB)
        assertEquals("Bearer $freshJwt", retriedA?.header("Authorization"))
        assertEquals("Bearer $freshJwt", retriedB?.header("Authorization"))
        assertEquals(1, api.fetchTokenCallCount)
        assertEquals(freshJwt, tokenStore.fetchToken())
    }

    private fun buildAnnotatedRequest(): Request {
        val method = AnnotatedApi::class.java.getMethod("annotatedMethod")
        return Request.Builder()
            .url("https://example.com/api/test")
            .tag(Invocation::class.java, Invocation.of(method, listOf<Any>()))
            .build()
    }

    private fun unauthorizedResponse(request: Request, prior: Response? = null): Response =
        Response.Builder()
            .request(request)
            .protocol(Protocol.HTTP_1_1)
            .code(401)
            .message("Unauthorized")
            .body("".toResponseBody())
            .apply { if (prior != null) priorResponse(prior) }
            .build()

    private interface AnnotatedApi {
        @CallWithBearerToken
        fun annotatedMethod()
    }
}
