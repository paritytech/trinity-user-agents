package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import okhttp3.Call
import okhttp3.Callback
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import okio.Timeout
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.Executors

private const val FACE = """{"tag":"String","value":{"text":"Loyalty"}}"""

class OkHttpRemoteFaceSourceTest {
    private val ioThread = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "face-io")
    }

    private val dispatchers = object : CoroutineDispatchers {
        override val main: CoroutineDispatcher = Dispatchers.Unconfined
        override val io: CoroutineDispatcher = ioThread.asCoroutineDispatcher()
        override val computation: CoroutineDispatcher = Dispatchers.Unconfined
    }

    private var callThread: String? = null
    private var responseCode = 200
    private var body: String = FACE

    private val calls = Call.Factory { request ->
        FakeCall(request) {
            callThread = Thread.currentThread().name
            Response.Builder()
                .request(request)
                .protocol(Protocol.HTTP_1_1)
                .code(responseCode)
                .message("fake")
                .body(body.toResponseBody("application/json".toMediaType()))
                .build()
        }
    }

    private val source = OkHttpRemoteFaceSource(
        calls = calls,
        dispatchers = dispatchers,
    )

    /**
     * `execute()` blocks. Called on the thread the ViewModel collects on, Android answers with
     * `NetworkOnMainThreadException` — which carries no message, so the screen can only say the face
     * could not be drawn and the real cause is lost.
     */
    @Test
    fun `the blocking call leaves the caller's thread`() = runBlocking {
        val caller = Thread.currentThread().name

        source.fetch("http://127.0.0.1:5173/face.json")

        // Coroutine debug mode appends "@coroutine#n" to the thread's name.
        assertTrue("ran on $callThread", callThread.orEmpty().startsWith("face-io"))
        assertNotEquals(caller, callThread)
    }

    @Test
    fun `a served face decodes`() = runBlocking {
        val face = source.fetch("http://127.0.0.1:5173/face.json")

        assertTrue("expected the face to decode, got ${face.exceptionOrNull()}", face.isSuccess)
    }

    /** A dev server answering 404 for a mistyped path must say so, not decode its error page. */
    @Test
    fun `a failing status is reported with its code`() = runBlocking {
        responseCode = 404
        body = "not found"

        val result = source.fetch("http://127.0.0.1:5173/missing.json")

        assertTrue(result.isFailure)
        assertTrue(
            "the message should name the status, was: ${result.exceptionOrNull()?.message}",
            result.exceptionOrNull()?.message.orEmpty().contains("404"),
        )
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
}
