package io.paritytech.polkadotapp.common.utils

import kotlinx.coroutines.CancellableContinuation
import kotlinx.coroutines.CompletionHandler
import kotlinx.coroutines.suspendCancellableCoroutine
import okhttp3.Call
import okhttp3.Callback
import okhttp3.RequestBody
import okhttp3.Response
import okio.Buffer
import okio.IOException
import kotlin.coroutines.resumeWithException

suspend inline fun Call.await(): Response {
    return suspendCancellableCoroutine { continuation ->
        val callback = ContinuationCallback(this, continuation)
        enqueue(callback)
        continuation.invokeOnCancellation(callback)
    }
}

class ContinuationCallback(
    private val call: Call,
    private val continuation: CancellableContinuation<Response>
) : Callback, CompletionHandler {
    override fun onResponse(call: Call, response: Response) {
        continuation.resume(response) { _, cancelledResponse, _ -> cancelledResponse.close() }
    }

    override fun onFailure(call: Call, e: IOException) {
        if (continuation.isActive) {
            continuation.resumeWithException(e)
        }
    }

    override fun invoke(cause: Throwable?) {
        try {
            call.cancel()
        } catch (_: Throwable) {
        }
    }
}

fun Iterable<String>.asQueryParam() = joinToString(separator = ",")

fun RequestBody.toByteArray() = Buffer().also { writeTo(it) }.readByteArray()
