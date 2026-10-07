package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import android.net.Uri
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor.WorkerScript
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import okhttp3.Call
import okhttp3.Callback
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import okio.Timeout
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

class WorkerModuleFetcherTest {
    private var responseCode = 200
    private var body = WASM_HEADER
    private var requestedUrl: String? = null

    private val calls = Call.Factory { request ->
        FakeCall(request) {
            requestedUrl = request.url.toString()
            Response.Builder()
                .request(request)
                .protocol(Protocol.HTTP_1_1)
                .code(responseCode)
                .message("fake")
                .body(body.toResponseBody("application/wasm".toMediaType()))
                .build()
        }
    }

    private val fetcher = WorkerModuleFetcher(calls, TestCoroutineDispatchers(Dispatchers.Unconfined))
    private lateinit var script: WorkerScript

    @Before
    fun parseScriptUrl() {
        mockkStatic(Uri::class)
        every { Uri.parse(SCRIPT_URL) } returns mockk {
            every { scheme } returns "https"
            every { host } returns "dev.example.invalid"
            every { port } returns -1
            every { path } returns "/worker/guest.wasm"
        }
        script = WorkerScript.of(SCRIPT_URL)
    }

    @After
    fun restoreUri() = unmockkStatic(Uri::class)

    @Test
    fun `a wasm body is returned as the module from the worker url`() = runBlocking<Unit> {
        assertArrayEquals(WASM_HEADER, fetcher.fetch(script).getOrThrow())
        assertEquals("https://dev.example.invalid/worker/guest.wasm", requestedUrl)
    }

    @Test
    fun `a body without the wasm magic is refused`() = runBlocking<Unit> {
        body = "export default {}".toByteArray()
        assertTrue(fetcher.fetch(script).exceptionOrNull() is WorkerModuleError.NotWasm)
    }

    @Test
    fun `an http failure is refused`() = runBlocking<Unit> {
        responseCode = 404
        assertTrue(fetcher.fetch(script).exceptionOrNull() is WorkerModuleError.Rejected)
    }

    @Test
    fun `an oversized body is refused`() = runBlocking<Unit> {
        body = ByteArray(9 * 1024 * 1024).also { WASM_HEADER.copyInto(it) }
        assertTrue(fetcher.fetch(script).exceptionOrNull() is WorkerModuleError.TooLarge)
    }

    private class FakeCall(private val request: Request, private val respond: () -> Response) : Call {
        override fun request(): Request = request
        override fun execute(): Response = respond()
        override fun enqueue(responseCallback: Callback) = throw UnsupportedOperationException()
        override fun cancel() = Unit
        override fun isExecuted(): Boolean = false
        override fun isCanceled(): Boolean = false
        override fun timeout(): Timeout = Timeout.NONE
        override fun clone(): Call = FakeCall(request, respond)
    }

    private companion object {
        const val SCRIPT_URL = "https://dev.example.invalid/worker/guest.wasm"
        val WASM_HEADER = byteArrayOf(0x00, 0x61, 0x73, 0x6D, 1, 0, 0, 0)
    }
}
