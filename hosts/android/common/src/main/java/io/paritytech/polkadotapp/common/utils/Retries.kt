package io.paritytech.polkadotapp.common.utils

import io.novasama.substrate_sdk_android.wsrpc.recovery.LinearReconnectStrategy
import io.novasama.substrate_sdk_android.wsrpc.recovery.ReconnectStrategy
import kotlinx.coroutines.delay
import timber.log.Timber
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

private const val MAX_RETRY_DOUBLINGS = 30

fun interface RetryDelayStrategy {
    fun delayFor(attempt: Int): Duration
}

fun exponentialRetryDelay(initialDelay: Duration, maxDelay: Duration): RetryDelayStrategy {
    return RetryDelayStrategy { attempt ->
        val doublings = (attempt - 1).coerceIn(0, MAX_RETRY_DOUBLINGS)
        val exponential = initialDelay * (1 shl doublings)
        exponential.coerceAtMost(maxDelay)
    }
}

suspend inline fun <T> retryUntilDone(
    retryStrategy: ReconnectStrategy = LinearReconnectStrategy(step = 500L),
    block: () -> T,
): T {
    val retryDelay = RetryDelayStrategy { attempt -> retryStrategy.getTimeForReconnect(attempt).milliseconds }
    return retryUntilSuccess(retryDelay) { runCatching { block() } }
}

suspend inline fun <T> retryUntilSuccess(
    retryDelay: RetryDelayStrategy,
    block: () -> Result<T>,
): T {
    return retryUntilSuccess(retryDelay, awaitRetry = { _, retryIn -> delay(retryIn) }, block)
}

/** [awaitRetry] waits before the next attempt; it may return before `retryIn` has passed to retry sooner. */
inline fun <T> retryUntilSuccess(
    retryDelay: RetryDelayStrategy,
    awaitRetry: (error: Throwable, retryIn: Duration) -> Unit,
    block: () -> Result<T>,
): T {
    var attempt = 0

    while (true) {
        block()
            .onSuccess { return it }
            .onFailure {
                Timber.w(it, "Failed to execute retriable operation:")

                attempt++

                awaitRetry(it, retryDelay.delayFor(attempt))
            }
    }
}
