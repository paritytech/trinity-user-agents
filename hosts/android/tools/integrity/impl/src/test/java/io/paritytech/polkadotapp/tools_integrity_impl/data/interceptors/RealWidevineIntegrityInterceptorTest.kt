package io.paritytech.polkadotapp.tools_integrity_impl.data.interceptors

import com.google.gson.Gson
import com.google.gson.JsonArray
import com.google.gson.JsonObject
import com.google.gson.JsonParser
import com.google.gson.JsonPrimitive
import io.paritytech.polkadotapp.common.utils.toByteArray
import io.paritytech.polkadotapp.tools_integrity_api.claim.ClaimDeviceEvidence
import io.paritytech.polkadotapp.tools_integrity_api.claim.ClaimDeviceEvidenceProvider
import io.paritytech.polkadotapp.tools_integrity_api.domain.error.IntegrityError
import io.paritytech.polkadotapp.tools_integrity_api.interceptors.CallWithWidevineIntegrity
import io.paritytech.polkadotapp.tools_integrity_impl.data.integrity.WidevineIntegrityParamsInjector
import kotlinx.coroutines.CancellationException
import okhttp3.Interceptor
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.ResponseBody
import okio.Buffer
import okio.BufferedSource
import okio.ForwardingSource
import okio.buffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import retrofit2.HttpException
import retrofit2.Invocation
import java.io.IOException
import java.security.KeyStoreException
import java.util.concurrent.TimeUnit
import retrofit2.Response as RetrofitResponse

class RealWidevineIntegrityInterceptorTest {
    @Test
    fun `injects all evidence fields while preserving the claim body`() {
        val provider = QueueEvidenceProvider(listOf(Result.success(initialEvidence)))
        val interceptor = createInterceptor(provider)
        val request = buildRequest(annotated = true)
        val chain = FakeChain(request, ResponseSpec(200, "{}"))

        val response = interceptor.intercept(chain)

        val sentRequest = chain.requests.single()
        val sentBody = sentRequest.jsonBody()
        assertEquals(JsonPrimitive("alice.07"), sentBody["username"])
        assertEquals(JsonPrimitive("sig"), sentBody["dotns"].asJsonObject["signature"])
        assertTrue(sentRequest.rawBody().contains(""""signedAt":1725000000"""))
        assertEquals(JsonPrimitive("challenge"), sentBody["deviceChallenge"])
        assertEquals(JsonPrimitive("device-id"), sentBody["deviceId"])
        assertEquals(
            JsonArray().apply {
                add("leaf")
                add("root")
            },
            sentBody["attestationChain"],
        )
        assertEquals(JSON_MEDIA_TYPE, sentRequest.body?.contentType())
        assertSame(request.tag(Invocation::class.java), sentRequest.tag(Invocation::class.java))
        response.close()
    }

    @Test
    fun `sends the original claim when evidence is unavailable`() {
        val provider = QueueEvidenceProvider(listOf(Result.success(null)))
        val interceptor = createInterceptor(provider)
        val request = buildRequest(annotated = true)
        val chain = FakeChain(request, ResponseSpec(200, "{}"))

        val response = interceptor.intercept(chain)

        assertSame(request, chain.requests.single())
        assertEquals(1, provider.calls)
        response.close()
    }

    @Test
    fun `ignores calls without the Widevine marker`() {
        val provider = QueueEvidenceProvider()
        val interceptor = createInterceptor(provider)
        val request = buildRequest(annotated = false)
        val chain = FakeChain(request, ResponseSpec(200, "{}"))

        val response = interceptor.intercept(chain)

        assertSame(request, chain.requests.single())
        assertEquals(0, provider.calls)
        response.close()
    }

    @Test
    fun `retries invalid evidence once with fresh evidence`() {
        val provider = QueueEvidenceProvider(
            listOf(
                Result.success(initialEvidence),
                Result.success(freshEvidence),
            )
        )
        val interceptor = createInterceptor(provider)
        val chain = FakeChain(
            buildRequest(annotated = true),
            ResponseSpec(403, INVALID_EVIDENCE_BODY),
            ResponseSpec(200, "{}"),
        )

        val response = interceptor.intercept(chain)

        assertEquals(2, chain.requests.size)
        assertEquals(JsonPrimitive("device-id"), chain.requests[0].jsonBody()["deviceId"])
        assertEquals(JsonPrimitive("fresh-device-id"), chain.requests[1].jsonBody()["deviceId"])
        assertEquals(JsonPrimitive("fresh-challenge"), chain.requests[1].jsonBody()["deviceChallenge"])
        assertEquals(2, provider.calls)
        assertTrue(chain.responses[0].trackingBody().closed)
        assertFalse(chain.responses[1].trackingBody().closed)
        response.close()
    }

    @Test
    fun `falls back to an evidence-less claim after the second invalid response`() {
        val provider = QueueEvidenceProvider(
            listOf(
                Result.success(initialEvidence),
                Result.success(freshEvidence),
            )
        )
        val originalRequest = buildRequest(annotated = true)
        val interceptor = createInterceptor(provider)
        val chain = FakeChain(
            originalRequest,
            ResponseSpec(403, INVALID_EVIDENCE_BODY),
            ResponseSpec(403, INVALID_EVIDENCE_BODY),
            ResponseSpec(200, "{}"),
        )

        val response = interceptor.intercept(chain)

        assertEquals(3, chain.requests.size)
        assertSame(originalRequest, chain.requests[2])
        assertFalse(chain.requests[2].jsonBody().has("deviceId"))
        assertEquals(2, provider.calls)
        assertTrue(chain.responses[0].trackingBody().closed)
        assertTrue(chain.responses[1].trackingBody().closed)
        response.close()
    }

    @Test
    fun `does not retry an unrelated error`() {
        assertNotRetried("""{"error":"SOMETHING_ELSE"}""")
    }

    @Test
    fun `does not retry a malformed error body`() {
        assertNotRetried("{")
    }

    @Test
    fun `does not retry a missing error field`() {
        assertNotRetried("{}")
    }

    @Test
    fun `does not retry a non-string error field`() {
        assertNotRetried("""{"error":42}""")
    }

    @Test
    fun `maps unknown evidence failures to unknown integrity`() {
        assertEvidenceFailureMapped(
            error = IllegalStateException("unexpected failure"),
            expected = IntegrityError.Unknown,
        )
    }

    @Test
    fun `preserves typed integrity failures`() {
        assertEvidenceFailureMapped(
            error = IntegrityError.AttestationRejected,
            expected = IntegrityError.AttestationRejected,
        )
    }

    @Test
    fun `maps transport failures to transient integrity`() {
        assertEvidenceFailureMapped(
            error = IOException("challenge unavailable"),
            expected = IntegrityError.AttestationTransient,
        )
    }

    @Test
    fun `maps backend challenge failures to transient integrity`() {
        val response = RetrofitResponse.error<Unit>(503, TrackingResponseBody("{}"))
        assertEvidenceFailureMapped(
            error = HttpException(response),
            expected = IntegrityError.AttestationTransient,
        )
    }

    @Test
    fun `maps Keystore failures to unavailable integrity`() {
        assertEvidenceFailureMapped(
            error = KeyStoreException("hardware attestation unavailable"),
            expected = IntegrityError.AttestationUnavailable,
        )
    }

    @Test
    fun `preserves evidence cancellation`() {
        val cancellation = CancellationException("claim cancelled")
        val provider = QueueEvidenceProvider(listOf(Result.failure(cancellation)))
        val interceptor = createInterceptor(provider)
        val chain = FakeChain(buildRequest(annotated = true), ResponseSpec(200, "{}"))

        val thrown = runCatching { interceptor.intercept(chain) }.exceptionOrNull()

        assertSame(cancellation, thrown)
        assertTrue(chain.requests.isEmpty())
    }

    private fun assertNotRetried(errorBody: String) {
        val provider = QueueEvidenceProvider(listOf(Result.success(initialEvidence)))
        val interceptor = createInterceptor(provider)
        val chain = FakeChain(buildRequest(annotated = true), ResponseSpec(403, errorBody))

        val response = interceptor.intercept(chain)

        assertEquals(1, chain.requests.size)
        assertEquals(1, provider.calls)
        assertSame(chain.responses.single(), response)
        assertEquals(errorBody, response.body.string())
    }

    private fun assertEvidenceFailureMapped(error: Throwable, expected: IntegrityError) {
        val provider = QueueEvidenceProvider(listOf(Result.failure(error)))
        val interceptor = createInterceptor(provider)
        val chain = FakeChain(buildRequest(annotated = true), ResponseSpec(200, "{}"))

        val thrown = runCatching { interceptor.intercept(chain) }.exceptionOrNull()

        assertTrue("Expected IOException but was $thrown", thrown is IOException)
        assertEquals(expected, thrown?.cause)
        assertTrue(chain.requests.isEmpty())
    }

    private fun createInterceptor(provider: ClaimDeviceEvidenceProvider): RealWidevineIntegrityInterceptor {
        return RealWidevineIntegrityInterceptor(WidevineIntegrityParamsInjector(provider, gson), gson)
    }

    private fun buildRequest(annotated: Boolean): Request {
        val body = """{"username":"alice.07","dotns":{"signature":"sig","signedAt":1725000000}}"""
        val builder = Request.Builder()
            .url("https://example.com/api/v1/usernames")
            .post(body.toRequestBody(JSON_MEDIA_TYPE))
        if (annotated) {
            val method = AnnotatedApi::class.java.getMethod("claim")
            builder.tag(Invocation::class.java, Invocation.of(method, emptyList<Any>()))
        }
        return builder.build()
    }

    private fun Request.rawBody(): String = requireNotNull(body).toByteArray().toString(Charsets.UTF_8)

    private fun Request.jsonBody(): JsonObject = JsonParser.parseString(rawBody()).asJsonObject

    private interface AnnotatedApi {
        @CallWithWidevineIntegrity
        fun claim()
    }

    private companion object {
        const val INVALID_EVIDENCE_BODY =
            """{"error":"DEVICE_EVIDENCE_INVALID","message":"stale evidence"}"""
        val gson = Gson()
        val initialEvidence = ClaimDeviceEvidence(
            attestationChain = listOf("leaf", "root"),
            deviceChallenge = "challenge",
            deviceId = "device-id",
        )
        val freshEvidence = ClaimDeviceEvidence(
            attestationChain = listOf("fresh-leaf", "fresh-root"),
            deviceChallenge = "fresh-challenge",
            deviceId = "fresh-device-id",
        )
    }
}

private class QueueEvidenceProvider(
    results: List<Result<ClaimDeviceEvidence?>> = emptyList(),
) : ClaimDeviceEvidenceProvider {
    private val results = ArrayDeque(results)
    var calls: Int = 0
        private set

    override suspend fun collectEvidence(): Result<ClaimDeviceEvidence?> {
        calls += 1
        return results.removeFirst()
    }
}

private class FakeChain(
    private val originalRequest: Request,
    vararg responseSpecs: ResponseSpec,
) : Interceptor.Chain {
    private val responseSpecs = ArrayDeque(responseSpecs.toList())
    val requests = mutableListOf<Request>()
    val responses = mutableListOf<Response>()

    override fun request(): Request = originalRequest

    override fun proceed(request: Request): Response {
        requests += request
        val spec = responseSpecs.removeFirst()
        return Response.Builder()
            .request(request)
            .protocol(Protocol.HTTP_1_1)
            .code(spec.code)
            .message("HTTP ${spec.code}")
            .body(TrackingResponseBody(spec.body))
            .build()
            .also(responses::add)
    }

    override fun connection() = null
    override fun call() = throw UnsupportedOperationException()
    override fun connectTimeoutMillis() = 0
    override fun withConnectTimeout(timeout: Int, unit: TimeUnit) = this
    override fun readTimeoutMillis() = 0
    override fun withReadTimeout(timeout: Int, unit: TimeUnit) = this
    override fun writeTimeoutMillis() = 0
    override fun withWriteTimeout(timeout: Int, unit: TimeUnit) = this
}

private class ResponseSpec(
    val code: Int,
    val body: String,
)

private class TrackingResponseBody(
    private val content: String,
) : ResponseBody() {
    var closed: Boolean = false
        private set
    private val bufferedSource = object : ForwardingSource(Buffer().writeUtf8(content)) {
        override fun close() {
            closed = true
            super.close()
        }
    }.buffer()

    override fun contentType() = JSON_MEDIA_TYPE

    override fun contentLength(): Long = content.toByteArray().size.toLong()

    override fun source(): BufferedSource = bufferedSource
}

private fun Response.trackingBody(): TrackingResponseBody = body as TrackingResponseBody

private val JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()
