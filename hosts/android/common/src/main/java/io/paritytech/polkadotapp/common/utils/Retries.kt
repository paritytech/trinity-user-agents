package io.paritytech.polkadotapp.common.utils

import io.novasama.substrate_sdk_android.wsrpc.recovery.LinearReconnectStrategy
import io.novasama.substrate_sdk_android.wsrpc.recovery.ReconnectStrategy
import kotlinx.coroutines.delay
import timber.log.Timber

suspend inline fun <T> retryUntilDone(
    retryStrategy: ReconnectStrategy = LinearReconnectStrategy(step = 500L),
    block: () -> T,
): T {
    var attempt = 0

    while (true) {
        val blockResult = runCatching { block() }

        blockResult
            .onSuccess { return it }
            .onFailure {
                Timber.w(it, "Failed to execute retriable operation:")

                attempt++

                delay(retryStrategy.getTimeForReconnect(attempt))
            }
    }
}
