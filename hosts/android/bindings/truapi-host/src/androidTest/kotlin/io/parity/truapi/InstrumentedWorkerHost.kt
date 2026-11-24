package io.parity.truapi

import uniffi.truapi.NativeWebViewWorkerHost
import uniffi.truapi.NativeWorkerAttempt
import uniffi.truapi.ProductId
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

internal class InstrumentedWorkerHost : NativeWebViewWorkerHost {
    private val started = CompletableFuture<NativeWorkerAttempt>()
    val stopped = CompletableFuture<ULong>()

    override fun start(productId: ProductId, attempt: NativeWorkerAttempt) {
        started.complete(attempt)
    }

    override fun stop(productId: ProductId, attemptId: ULong) {
        stopped.complete(attemptId)
    }

    fun awaitAttempt(): NativeWorkerAttempt = started.get(10, TimeUnit.SECONDS)
}
