package io.paritytech.polkadotapp.common.data.network

import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import okhttp3.Interceptor
import okhttp3.Request
import org.junit.Assert.assertEquals
import org.junit.Test

class OverrideBaseUrlInterceptorTest {
    @Test
    fun `keeps the resolved base url path`() {
        val rewritten = intercept(
            baseUrl = "https://polkadot-test.substrate.dev/dub/",
            requestUrl = "https://$PLACEHOLDER/api/v1/attester?id=1",
        )

        assertEquals("https://polkadot-test.substrate.dev/dub/api/v1/attester?id=1", rewritten)
    }

    @Test
    fun `keeps the resolved base url path without trailing slash`() {
        val rewritten = intercept(
            baseUrl = "https://polkadot-test.substrate.dev/dub",
            requestUrl = "https://$PLACEHOLDER/api/v1/attester",
        )

        assertEquals("https://polkadot-test.substrate.dev/dub/api/v1/attester", rewritten)
    }

    @Test
    fun `rewrites host and port for a root base url`() {
        val rewritten = intercept(
            baseUrl = "http://localhost:8092/",
            requestUrl = "https://$PLACEHOLDER/api/v1/attester",
        )

        assertEquals("http://localhost:8092/api/v1/attester", rewritten)
    }

    @Test
    fun `leaves other hosts untouched`() {
        val rewritten = intercept(
            baseUrl = "https://polkadot-test.substrate.dev/dub/",
            requestUrl = "https://example.com/api/v1/attester",
        )

        assertEquals("https://example.com/api/v1/attester", rewritten)
    }

    private fun intercept(baseUrl: String, requestUrl: String): String {
        val interceptor = OverrideBaseUrlInterceptor(PLACEHOLDER) { baseUrl }
        val proceeded = slot<Request>()
        val chain = mockk<Interceptor.Chain> {
            every { request() } returns Request.Builder().url(requestUrl).build()
            every { proceed(capture(proceeded)) } returns mockk()
        }

        interceptor.intercept(chain)

        return proceeded.captured.url.toString()
    }

    private companion object {
        const val PLACEHOLDER = "placeholder.sentinel"
    }
}
