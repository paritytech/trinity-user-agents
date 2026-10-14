@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.common.domain.model

import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * Context receiver carrying "what time is it right now". Code that needs `now()` should
 * declare `context(CurrentTimeContext)` rather than calling `Clock.System.now()` directly
 * — that way tests inject a fake without static mocking, and the dependency is explicit
 * at the call site.
 */
fun interface CurrentTimeContext {
    fun currentTime(): Instant
}
