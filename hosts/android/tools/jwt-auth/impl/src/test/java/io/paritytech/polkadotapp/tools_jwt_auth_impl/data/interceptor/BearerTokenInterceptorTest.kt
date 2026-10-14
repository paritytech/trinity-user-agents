package io.paritytech.polkadotapp.tools_jwt_auth_impl.data.interceptor

import com.google.gson.Gson
import io.paritytech.polkadotapp.tools_jwt_auth_api.CallWithBearerToken
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.FakeTimeProvider
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.api.AuthTokenApi
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.createFakeTokenStore
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.makeValidJWT
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.manager.JWTTokenProvider
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.model.JwtRequest
import io.paritytech.polkadotapp.tools_jwt_auth_impl.data.parser.JWTParser
import kotlinx.coroutines.runBlocking
import okhttp3.Interceptor
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.`when`
import retrofit2.HttpException
import retrofit2.Invocation
import java.io.IOException
import retrofit2.Response as RetrofitResponse

class BearerTokenInterceptorTest {
    @Test
    fun `adds Authorization header when annotation present`() {
        val storedJwt = makeValidJWT(exp = 2_000_000_000L)
        val interceptor = BearerTokenInterceptor(providerWithStoredToken(storedJwt))

        val request = buildRequest(annotated = true)
        val chain = FakeChain(request)

        interceptor.intercept(chain)

        assertEquals("Bearer $storedJwt", chain.proceedRequest?.header("Authorization"))
    }

    @Test
    fun `does not add header when annotation absent`() {
        val interceptor = BearerTokenInterceptor(providerWithStoredToken(makeValidJWT(2_000_000_000L)))

        val request = buildRequest(annotated = false)
        val chain = FakeChain(request)

        interceptor.intercept(chain)

        assertNull(chain.proceedRequest?.header("Authorization"))
    }

    @Test
    fun `wraps non-IOException from token acquisition as IOException`() {
        val httpException = HttpException(RetrofitResponse.error<Any>(401, "".toResponseBody()))
        val interceptor = BearerTokenInterceptor(providerWithFailingFetch(httpException))
        val chain = FakeChain(buildRequest(annotated = true))

        val thrown = runCatching { interceptor.intercept(chain) }.exceptionOrNull()

        assertNotNull("intercept() should have thrown", thrown)
        assertTrue("Expected IOException, got ${thrown!!::class.qualifiedName}", thrown is IOException)
        assertSame(httpException, thrown.cause)
    }

    @Test
    fun `passes IOException from token acquisition through unchanged`() {
        val ioe = IOException("transport down")
        val interceptor = BearerTokenInterceptor(providerWithFailingFetch(ioe))
        val chain = FakeChain(buildRequest(annotated = true))

        val thrown = runCatching { interceptor.intercept(chain) }.exceptionOrNull()

        assertSame(ioe, thrown)
    }

    @Test
    fun `preserves existing headers`() {
        val storedJwt = makeValidJWT(exp = 2_000_000_000L)
        val interceptor = BearerTokenInterceptor(providerWithStoredToken(storedJwt))

        val request = buildRequest(annotated = true)
            .newBuilder()
            .addHeader("Content-Type", "application/json")
            .build()
        val chain = FakeChain(request)

        interceptor.intercept(chain)

        assertEquals("application/json", chain.proceedRequest?.header("Content-Type"))
        assertEquals("Bearer $storedJwt", chain.proceedRequest?.header("Authorization"))
    }

    private fun providerWithStoredToken(token: String): JWTTokenProvider {
        val tokenStore = createFakeTokenStore()
        tokenStore.saveToken(token)
        return JWTTokenProvider(
            tokenStore = tokenStore,
            authTokenApi = mock(AuthTokenApi::class.java),
            timeProvider = FakeTimeProvider(1_000_000_000L),
            jwtParser = JWTParser(Gson()),
            gson = Gson(),
        )
    }

    private fun providerWithFailingFetch(failure: Throwable): JWTTokenProvider {
        val authTokenApi = mock(AuthTokenApi::class.java)
        runBlocking {
            if (failure is RuntimeException) {
                `when`(authTokenApi.fetchToken(JwtRequest)).thenThrow(failure)
            } else {
                `when`(authTokenApi.fetchToken(JwtRequest)).then { throw failure }
            }
        }
        return JWTTokenProvider(
            tokenStore = createFakeTokenStore(),
            authTokenApi = authTokenApi,
            timeProvider = FakeTimeProvider(1_000_000_000L),
            jwtParser = JWTParser(Gson()),
            gson = Gson(),
        )
    }

    private fun buildRequest(annotated: Boolean): Request {
        val builder = Request.Builder().url("https://example.com/api/test")
        if (annotated) {
            val method = AnnotatedApi::class.java.getMethod("annotatedMethod")
            builder.tag(Invocation::class.java, Invocation.of(method, listOf<Any>()))
        }
        return builder.build()
    }

    private interface AnnotatedApi {
        @CallWithBearerToken
        fun annotatedMethod()
    }
}

private class FakeChain(private val request: Request) : Interceptor.Chain {
    var proceedRequest: Request? = null

    override fun request(): Request = request

    override fun proceed(request: Request): Response {
        proceedRequest = request
        return Response.Builder()
            .request(request)
            .protocol(Protocol.HTTP_1_1)
            .code(200)
            .message("OK")
            .body("".toResponseBody())
            .build()
    }

    override fun connection() = null
    override fun call() = throw UnsupportedOperationException()
    override fun connectTimeoutMillis() = 0
    override fun withConnectTimeout(timeout: Int, unit: java.util.concurrent.TimeUnit) = this
    override fun readTimeoutMillis() = 0
    override fun withReadTimeout(timeout: Int, unit: java.util.concurrent.TimeUnit) = this
    override fun writeTimeoutMillis() = 0
    override fun withWriteTimeout(timeout: Int, unit: java.util.concurrent.TimeUnit) = this
}
