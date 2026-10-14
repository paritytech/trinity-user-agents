package io.paritytech.polkadotapp.common.utils

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import okhttp3.Call
import okhttp3.Callback
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.InterruptedIOException

class HttpTest {
    private lateinit var callback: Callback
    private val call = mockk<Call>(relaxed = true).apply {
        every { enqueue(any()) } answers { callback = firstArg() }
    }

    @Test
    fun `closes response when cancellation wins before the caller resumes`() = runBlocking<Unit> {
        val body = mockk<ResponseBody>(relaxed = true)
        val response = Response.Builder()
            .request(Request.Builder().url("https://example.com").build())
            .protocol(Protocol.HTTP_1_1).code(200).message("OK").body(body).build()
        val request = launch(start = CoroutineStart.UNDISPATCHED) { call.await().use { } }

        callback.onResponse(call, response)
        request.cancelAndJoin()

        verify(exactly = 1) { body.close() }
    }

    @Test
    fun `propagates timeout when the call is canceled but the caller is active`() = runBlocking<Unit> {
        every { call.isCanceled() } returns true
        val timeout = InterruptedIOException("timeout")
        val request = async(start = CoroutineStart.UNDISPATCHED) { runCatching { call.await() } }
        try {
            callback.onFailure(call, timeout)
            yield()

            assertTrue("A canceled HTTP call must still deliver its failure", request.isCompleted)
            assertEquals(timeout, request.await().exceptionOrNull())
        } finally {
            request.cancelAndJoin()
        }
    }
}
