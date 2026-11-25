package io.paritytech.polkadotapp.app

import uniffi.truapi.NativeWorkerAttempt
import uniffi.truapi.NativeWorkerEngine
import uniffi.truapi.ProductId

/** A worker engine for a runtime whose test starts no worker. */
internal object NoWorkers : NativeWorkerEngine {
    override fun start(productId: ProductId, attempt: NativeWorkerAttempt) = Unit

    override fun stop(productId: ProductId, attemptId: ULong) = Unit
}
