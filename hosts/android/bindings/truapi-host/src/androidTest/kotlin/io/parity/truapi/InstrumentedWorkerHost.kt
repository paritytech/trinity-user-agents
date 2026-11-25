package io.parity.truapi

import uniffi.truapi.NativeWorkerEngine
import uniffi.truapi.NativeWorkerAttempt
import uniffi.truapi.ProductId
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

internal class InstrumentedWorkerHost : NativeWorkerEngine {
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

/** A worker engine for a runtime whose test starts no worker. */
internal object NoWorkers : NativeWorkerEngine {
    override fun start(productId: ProductId, attempt: NativeWorkerAttempt) = Unit

    override fun stop(productId: ProductId, attemptId: ULong) = Unit
}
