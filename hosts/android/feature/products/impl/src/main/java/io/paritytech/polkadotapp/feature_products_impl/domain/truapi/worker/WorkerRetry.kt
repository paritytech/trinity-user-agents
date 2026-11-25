package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.retryWhen
import timber.log.Timber
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

/**
 * A stream that fails is opened again, for as long as it is collected. Ending instead would leave
 * what is on screen static for the worker's whole life: a new render is only opened when a
 * different execution is published, and a stop publishes none. The backoff counts every failure
 * of one collection and does not shrink after a successful draw.
 */
fun <T> Flow<T>.reopenAfterFailure(what: String): Flow<T> = retryWhen { failure, attempt ->
    Timber.w(failure, "%s ended, reopening", what)
    delay(reopenBackoff(attempt))
    true
}

/** Doubling from [BACKOFF_BASE] to [MAX_BACKOFF]: a slow surface is picked up at once, a broken one is not asked on a loop. */
fun reopenBackoff(attempt: Long): Duration =
    minOf(BACKOFF_BASE * (1 shl attempt.coerceAtMost(MAX_BACKOFF_SHIFT).toInt()), MAX_BACKOFF)

private val BACKOFF_BASE = 1.seconds
private val MAX_BACKOFF = 30.seconds
private const val MAX_BACKOFF_SHIFT = 5L
