package io.paritytech.polkadotapp.common.utils.network

import io.paritytech.polkadotapp.common.utils.RetryDelayStrategy
import io.paritytech.polkadotapp.common.utils.exponentialRetryDelay
import kotlinx.coroutines.delay
import kotlin.time.Duration.Companion.seconds

sealed interface NetworkRetryPolicy {
    data object UntilCancelled : NetworkRetryPolicy

    data class Attempts(val count: Int) : NetworkRetryPolicy

    companion object {
        val DEFAULT: NetworkRetryPolicy = Attempts(3)
    }
}

suspend fun <T> NetworkStateService.retrying(
    policy: NetworkRetryPolicy,
    isRetryable: (Throwable) -> Boolean,
    attempt: suspend () -> Result<T>,
): Result<T> {
    var failedAttempts = 0

    while (true) {
        val result = attempt()
        val error = result.exceptionOrNull() ?: return result
        if (!isRetryable(error)) return result

        if (!isNetworkAvailable.value) {
            awaitNetworkAvailable()
            continue
        }

        failedAttempts++
        if (policy is NetworkRetryPolicy.Attempts && failedAttempts >= policy.count) return result

        delay(NETWORK_RETRY_DELAY.delayFor(failedAttempts))
    }
}

private val NETWORK_RETRY_DELAY: RetryDelayStrategy = exponentialRetryDelay(initialDelay = 1.seconds, maxDelay = 30.seconds)
