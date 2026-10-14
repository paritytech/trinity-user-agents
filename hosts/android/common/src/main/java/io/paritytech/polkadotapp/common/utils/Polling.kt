package io.paritytech.polkadotapp.common.utils

import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlin.time.Duration

suspend inline fun runPolling(pollingInterval: Duration, crossinline poll: suspend () -> Unit) = coroutineScope {
    while (isActive) {
        poll()

        delay(pollingInterval)
    }
}
