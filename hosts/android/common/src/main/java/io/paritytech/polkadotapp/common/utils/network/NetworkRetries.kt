package io.paritytech.polkadotapp.common.utils.network

import kotlinx.coroutines.delay
import kotlin.time.Duration
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

        delay(retryDelayAfter(failedAttempts))
    }
}

private val FIRST_RETRY_DELAY = 1.seconds
private val MAX_RETRY_DELAY = 30.seconds

private const val MAX_DELAY_DOUBLINGS = 5

private fun retryDelayAfter(failedAttempts: Int): Duration {
    val doublings = (failedAttempts - 1).coerceAtMost(MAX_DELAY_DOUBLINGS)
    return (FIRST_RETRY_DELAY * (1 shl doublings)).coerceAtMost(MAX_RETRY_DELAY)
}
